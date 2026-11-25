"""Simulation of the live sync rules proposed in DESIGN.md (behavior only, no media).

Everything is in seconds. A chunk is (arrival, pts, track). Delivery delay d = arrival - pts.
Run: python3 design_sim.py [rtmp] [hls] [moq] [av] [misc] [review]
`review` (the scenarios of section 14) runs only when it is named.
"""
import bisect
import heapq
import math
import sys
from collections import deque
from dataclasses import dataclass, field


@dataclass
class Cfg:
    lower: float = 1.0
    start: float = 2.0
    upper: float = 5.0
    enforce: bool = False       # upper limit wins (measured on the reference track)
    segmented: bool = False     # hint: content is delivered in segments
    stitch: bool = False        # stage 0: pts discontinuities are stitched
    settle: float = 0.5
    tol: float = 0.1
    w_base: float = 1.0         # shortest confirmation window of the edge
    w_max: float = 60.0
    big_fall: float = 1.0       # a fall of the edge larger than this needs confirmation
    keep_min: float = 10.0
    mem: float = 120.0
    bucket: float = 0.25
    gap_min: float = 0.5        # first sight: a burst after a gap at least this long
    max_slew_time: float = 30.0
    rate_max: float = 0.04
    rate_min: float = 0.01
    rate_tau: float = 5.0
    start_timeout: float = 10.0
    idle: float = 10.0          # a track silent for this long stops counting
    ahead: float = 0.1
    frame: float = 0.02
    fwd_thr: float = 10.0
    back_thr: float = 1.0
    # only for the model of the current implementation (simulate_today)
    stab_tol: float = 0.1       # stabilization tolerance
    stale: float = 2.0          # stale estimate threshold
    max_wait: float = 0.0       # 0: twice the starting buffer


class Edge:
    """Fastest delivery. Falls at once, rises only when the whole window is later and flat."""

    def __init__(self, cfg):
        self.cfg = cfg
        self.samples = deque()      # (arrival, d) inside the window
        self.away = 0.0             # how long regular lateness lasts, set by the caller
        self.value = None
        self.last_arrival = None
        self.last4 = deque(maxlen=4)
        self.mark = None
        self.last_fall = None
        self.pending = []
        self.events = []            # ("rise" | "back", delta, since)
        self.risen = []             # rises not taken back: [delta, since, edge before, seen high]

    def base_window(self):
        """W: twice as long as regular lateness lasts."""
        return min(self.cfg.w_max, max(self.cfg.w_base, 2 * self.away))

    def window(self, now):
        """W, or back to the fourth chunk from the end on a track that delivers less."""
        w = self.base_window()
        if len(self.last4) == 4:
            w = max(w, now - self.last4[0])
        return min(self.cfg.w_max, w)

    def observe(self, a, d):
        cfg = self.cfg
        self.last_arrival = a
        self.last4.append(a)
        if self.value is None:
            self.value, self.mark, self.last_fall = d, d, a
            self.samples.append((a, d))
            return
        if d < self.value - cfg.big_fall:
            self.pending.append((a, d))
            if len(self.pending) < 3:
                return
            self.value = min(x for _, x in self.pending)
            self.samples = deque(self.pending)
            self.pending = []
        else:
            self.pending = []
            self.samples.append((a, d))
            self.value = min(self.value, d)
        # a rise is taken back when the edge returns at once to where it was before it
        while self.risen and self.value <= self.risen[-1][2] + cfg.tol:
            delta, since, _, high = self.risen.pop()
            if a - high <= self.window(a):
                self.events.append(("back", delta, since))
        for r in self.risen:
            if self.value >= r[2] + r[0] / 2:
                r[3] = a
        if self.value < self.mark - cfg.tol:
            self.mark, self.last_fall = self.value, a

    def candidate(self, now):
        """(c, arrival): the smallest delivery delay among the chunks of the last W."""
        since, best = now - self.base_window(), None
        for a, d in self.samples:
            if a >= since and (best is None or d < best[0] - 1e-9):
                best = (d, a)
        return best

    def tick(self, now):
        """ED3: both halves of the window are away from the edge, the later one not faster."""
        if self.value is None:
            return
        w = self.window(now)
        while self.samples and self.samples[0][0] < now - w:
            self.samples.popleft()
        m1 = m2 = math.inf
        for a, d in self.samples:
            if a < now - w / 2:
                m1 = min(m1, d)
            else:
                m2 = min(m2, d)
        c = min(m1, m2)
        if m1 < math.inf and m2 < math.inf and c > self.value + self.cfg.tol \
                and m2 >= m1 - self.cfg.tol:
            self.events.append(("rise", c - self.value, now - w - 0.05))
            self.risen.append([c - self.value, now - w - 0.05, self.value, now])
            self.value, self.mark = c, c

    def settled(self, now):
        return self.last_fall is not None and now - self.last_fall >= self.cfg.settle


class Depth:
    """Lateness that repeats: reached twice -> regular; kept for twice its longest period."""

    def __init__(self, cfg):
        self.cfg = cfg
        self.buckets = {}           # index -> [max d, min d]
        self.first_sight = None     # (time, d, gap): first chunk after the latest gap
        self.cache = (None, None)

    def observe(self, a, d, gap):
        i = int(a / self.cfg.bucket)
        b = self.buckets.get(i)
        if b is None:
            self.buckets[i] = [d, d]
            self.cache = (None, None)
        elif d > b[0] or d < b[1]:
            b[0], b[1] = max(b[0], d), min(b[1], d)
            self.cache = (None, None)
        if self.cfg.segmented and gap is not None and gap >= self.cfg.gap_min:
            self.first_sight = (a, d, gap)

    def shift(self, delta, before):
        """The edge rose by `delta` (or the rise is taken back, with `-delta`): what was
        measured before the window moves with it."""
        limit = int(before / self.cfg.bucket)
        for i, b in self.buckets.items():
            if i < limit:
                b[0] += delta
                b[1] += delta
        if self.first_sight and self.first_sight[0] < before:
            t, d, g = self.first_sight
            self.first_sight = (t, d + delta, g)
        self.cache = (None, None)

    def slow(self, now, edge):
        """(L, A): the highest regular lateness, and how long regular lateness lasts."""
        key = (int(now / self.cfg.bucket), round(edge, 4))
        if self.cache[0] != key:
            self.cache = (key, self.regular(key[0], edge))
        best, away = self.cache[1]
        if self.first_sight:
            t, d, gap = self.first_sight
            if now - t <= max(self.cfg.keep_min, 2 * (d - edge)):
                best = max(best, d)
        return best - edge, away

    def occasions(self, idx, level, h):
        """LT2: (last bucket, highest delay) of every separate occasion on which `level` was
        reached, within the margin `h`."""
        found, k = [], len(idx) - 1
        while True:
            while k >= 0 and self.buckets[idx[k]][0] < level - h:
                k -= 1
            if k < 0:
                break
            end, peak = idx[k], self.buckets[idx[k]][0]
            k -= 1                  # delivery was clearly faster in between: an older occasion
            while k >= 0 and self.buckets[idx[k]][1] >= level - 2 * h:
                peak = max(peak, self.buckets[idx[k]][0])
                k -= 1
            if k >= 0:
                peak = max(peak, self.buckets[idx[k]][0])
            k -= 1
            found.append((end, peak))
        found.reverse()
        return found

    def kept(self, at, last, i_now):
        """LT3, LT4: occasions at `at`, the latest two close enough, and not longer ago than
        twice the longest time between two of them."""
        cfg = self.cfg
        pair = cfg.mem / 2 / cfg.bucket
        if len(at) < 2 or at[-1] - at[-2] > pair:
            return False
        longest = max(b - a for a, b in zip(at, at[1:]) if b - a <= pair)
        return i_now - last <= max(cfg.keep_min / cfg.bucket, 2 * longest)

    def regular(self, i_now, edge):
        """(highest regular level, how long regular lateness lasts), read from the history."""
        cfg = self.cfg
        for i in [i for i in self.buckets if i < i_now - int(cfg.mem / cfg.bucket)]:
            del self.buckets[i]
        idx = sorted(self.buckets)
        best = edge
        for level in sorted({self.buckets[i][0] for i in idx if self.buckets[i][0] > edge},
                            reverse=True):
            found = self.occasions(idx, level, max(0.01, 0.1 * (level - edge)))
            if self.kept([end for end, _ in found], found[-1][0], i_now):
                # what two of them reached: the second highest
                best = min(level, sorted(peak for _, peak in found)[-2])
                break
        # LT9, how long delivery stays away from the edge: the same rule on durations
        runs, last = [], None       # (index of the chunk at the edge before, of the one after)
        for k, i in enumerate(idx):
            if self.buckets[i][1] <= edge + cfg.tol:
                if last is not None and k - last > 1:
                    runs.append((idx[last], i))
                last = k
        if last is not None and last < len(idx) - 1:
            runs.append((idx[last], i_now))
        away = 0.0
        for length in sorted({e - s for s, e in runs}, reverse=True):
            hit = [r for r in runs if r[1] - r[0] >= length - max(1.0, 0.1 * length)]
            if self.kept([r[0] for r in hit], hit[-1][1], i_now):
                away = min(length, sorted(r[1] - r[0] for r in hit)[-2]) * cfg.bucket
                break
        return best, away


class Track:
    def __init__(self, cfg, name):
        self.cfg, self.name = cfg, name
        self.edge, self.depth = Edge(cfg), Depth(cfg)
        self.buf = []               # (pts, arrival), ordered by pts
        self.played_until = -math.inf
        self.counts = False         # measured once: its burst has ended
        self.first_arrival = None
        self.offset = 0.0           # stage 0
        self.last_raw = None        # (raw pts, arrival, stitched pts)
        self.played = self.dropped = self.shown_late = self.dry_time = 0.0
        self.due, self.aired = [], False    # when the released chunks are on the output

    def schedule(self, play):
        heapq.heappush(self.due, play)
        self.played += self.cfg.frame
        self.played_until = max(self.played_until, play + self.cfg.frame)

    def on_air(self, now):
        """Whether its content is on the output now; None before the first chunk was. A chunk
        counts for a quarter longer than its length, so that slewed playback and a source with
        a fast clock have no holes."""
        while self.due and self.due[0] + 1.25 * self.cfg.frame <= now:
            heapq.heappop(self.due)
        if self.due and self.due[0] <= now:
            self.aired = True
        return self.due[0] <= now if self.due and self.aired else (False if self.aired else None)


@dataclass
class Result:
    log: list = field(default_factory=list)
    trace: list = field(default_factory=list)      # (time, buffer of the edge)
    tracks: dict = field(default_factory=dict)
    first_play: float = None
    jumps: int = 0
    free_jumps: int = 0
    slew_time: float = 0.0
    interrupted_time: float = 0.0


def simulate(cfg, chunks, t_end, reference=None):
    """chunks: (arrival, pts, track name), sorted by arrival."""
    names = sorted({c[2] for c in chunks})
    tracks = {n: Track(cfg, n) for n in names}
    reference = reference or ("audio" if "audio" in tracks else names[0])
    res = Result(tracks=tracks)
    T = T0 = E_prev = None      # T0: the timeline without what was added here (DC7)
    followed = unfollowed = 0.0  # rises of the edge that T has / has not followed yet
    moving, returning = 0, False
    i, now, dt = 0, 0.0, 0.01

    def log(text):
        res.log.append((round(now, 2), text))

    def edge_events(tr):
        """LT7: what was measured follows a rise of the edge; a rise that is taken back was
        lateness. DC7: the re-anchor that followed such a rise added latency after all."""
        nonlocal T0, followed, unfollowed
        for kind, delta, before in tr.edge.events:
            if kind == "rise":
                tr.depth.shift(delta, before)
                log(f"{tr.name}: edge +{delta:.2f}s")
            else:
                tr.depth.shift(-delta, before)
                log(f"{tr.name}: edge back, the rise of {delta:.2f}s was lateness")
                if T0 is not None and tr.counts:
                    u = min(delta, unfollowed)
                    f = min(delta - u, followed)
                    T0, followed, unfollowed = T0 - f, followed - f, unfollowed - u
        tr.edge.events.clear()

    while now < t_end:
        # ---- arrivals: stage 0 (stitch) and stage 1 (measure)
        while i < len(chunks) and chunks[i][0] <= now:
            a, raw, name = chunks[i]
            i += 1
            tr = tracks[name]
            if cfg.stitch and tr.last_raw is not None:
                praw, pa, pst = tr.last_raw
                if raw - praw < -cfg.back_thr or (raw - praw) - (a - pa) > cfg.fwd_thr:
                    new_offset = pst + cfg.frame - raw
                    log(f"{name}: discontinuity {new_offset - tr.offset:+.2f}s stitched")
                    tr.offset = new_offset
            p = raw + tr.offset
            tr.last_raw = (raw, a, p)
            gap = None if tr.edge.last_arrival is None else a - tr.edge.last_arrival
            tr.edge.observe(a, a - p)
            edge_events(tr)
            if tr.counts:
                tr.depth.observe(a, a - p, gap)
            bisect.insort(tr.buf, (p, a))
            if tr.first_arrival is None:
                tr.first_arrival = a
        delivering = [t for t in tracks.values() if t.first_arrival is not None]
        if not delivering:
            now += dt
            continue
        for tr in delivering:
            if tr.counts:
                tr.edge.away = tr.depth.slow(now, tr.edge.value)[1]
            tr.edge.tick(now)
            edge_events(tr)
            if not tr.counts and T is not None and (
                    tr.edge.settled(now) or now - tr.first_arrival >= cfg.start_timeout):
                tr.counts = True

        # ---- start
        if T is None:
            ready = all(t.edge.settled(now) for t in delivering)
            timeout = now - min(t.first_arrival for t in delivering) >= cfg.start_timeout
            if not (ready or timeout):
                now += dt
                continue
            for tr in delivering:
                tr.counts = True
            E = max(t.edge.value for t in delivering)
            if cfg.enforce and reference in tracks and tracks[reference].counts:
                E = min(E, tracks[reference].edge.value + cfg.upper - cfg.start)
            T = T0 = E + cfg.start
            res.first_play = now
            log(f"start: buffer {cfg.start:.2f}s" + ("" if ready else " (timeout)"))

        # ---- stage 2: combine and decide
        active = [t for t in tracks.values()
                  if t.counts and now - t.edge.last_arrival <= cfg.idle] or \
                 [t for t in tracks.values() if t.counts]
        E = max(t.edge.value for t in active)
        D = max(t.edge.value + t.depth.slow(now, t.edge.value)[0] for t in active)
        P = D - E
        Bs = max(cfg.start, P + cfg.lower)
        T_star, T_lo, T_hi = E + Bs, D + cfg.lower, E + cfg.upper + (Bs - cfg.start)
        cand = max((t.edge.candidate(now) or (t.edge.value, 0))[0] for t in active)
        T_cand = cand + Bs
        ref = tracks.get(reference)
        if ref not in active:       # TR5: the reference is audio while it counts
            ref = active[0]
        D_must = D                  # packets that must not be lost: a jump is worth it for them
        if cfg.enforce:
            D_must = ref.edge.value + ref.depth.slow(now, ref.edge.value)[0]
            cap = ref.edge.value + cfg.upper
            T_star, T_lo, T_hi = min(T_star, cap), min(T_lo, cap), min(T_hi, cap)
            T_cand = min(T_cand, (ref.edge.candidate(now) or (ref.edge.value, 0))[0] + cfg.upper)
        # DC7 bookkeeping: a re-anchor that follows a rise of the edge adds no latency, unless
        # the rise is taken back
        if E_prev is not None and E > E_prev:
            f = min(E - E_prev, T - T0)
            T0, followed, unfollowed = T0 + f, followed + f, unfollowed + E - E_prev - f
        E_prev = E
        interrupted = all(t.played_until <= now for t in tracks.values())
        err, T_was = T_star - T, T
        out = T < T_lo - cfg.tol or T > T_hi + cfg.tol
        if moving and err * moving <= 0.002:
            moving = 0              # reached, or the target is on the other side by now
        back = min(T - T0, -err)    # added here and above the target
        if interrupted:
            if out or moving > 0:   # a slew to later becomes a jump; one to earlier waits
                T = T_star
                res.free_jumps += 1
                moving = 0
                log(f"interrupted: re-anchored {err:+.2f}s -> buffer {T - E:.2f}s")
        elif moving or out:
            jump_thr = cfg.rate_max * cfg.max_slew_time
            late = err > 0 and T < D_must - cfg.tol
            if late or T - T_hi > jump_thr:
                why = "regular packets late" if late else "too far to slew"
                T = T_star
                res.jumps += 1
                moving = 0
                log(f"jump {err:+.2f}s -> buffer {T - E:.2f}s ({why})")
            else:
                if not moving:
                    log(f"slew {err:+.2f}s from buffer {T - E:.2f}s")
                moving = 1 if err > 0 else -1
                rate = min(cfg.rate_max, max(cfg.rate_min, abs(err) / cfg.rate_tau))
                T += math.copysign(min(abs(err), rate * dt), err)
                res.slew_time += dt
            returning = False
        elif returning or back > cfg.tol:
            if not returning:
                log(f"slew back {-back:+.2f}s from buffer {T - E:.2f}s (added earlier)")
            returning = back > 0.002
            rate = min(cfg.rate_max, max(cfg.rate_min, back / cfg.rate_tau))
            T -= min(max(back, 0.0), rate * dt)
            res.slew_time += dt
        if T > T_was:
            f = min(T - T_was, unfollowed)
            T0, followed, unfollowed = T0 + f, followed + f, unfollowed - f
        T0 = min(T0, T)

        # ---- stage 3: release
        air = [tr.on_air(now) for tr in tracks.values()]
        if not any(air) and False in air:
            res.interrupted_time += dt
        for tr, on in zip(tracks.values(), air):
            if on is False:
                tr.dry_time += dt
            while tr.buf:
                p, a = tr.buf[0]
                play = p + T
                if play - now > cfg.ahead:
                    break
                if play >= now:
                    del tr.buf[0]
                    tr.schedule(play)
                    continue
                if interrupted and T_cand > T + cfg.tol and p + T_cand >= now:
                    break           # kept: a restart at the candidate edge would still play it
                del tr.buf[0]
                if cfg.enforce and tr is not ref:
                    tr.shown_late += cfg.frame
                else:
                    tr.dropped += cfg.frame
        res.trace.append((now, T - E))
        now += dt
    return res


# ------------------------------------------------- the current implementation, for comparison

TODAY = None    # {"variant": "master" | "branch", "lookback": seconds}, from the command line


class TodayEstimator:
    """Model of `LiveEdgeEstimator`: the smallest and the largest delivery delay of a window."""
    BUCKET = 2.0

    def __init__(self, cfg, lookback):
        self.cfg, self.lookback = cfg, lookback
        self.reset()

    def reset(self):
        self.buckets = deque()      # [index, min, max of the chunks seen while stable, max gap]
        self.first = self.last = self.ref = self.since = None

    def stable(self, now):
        return self.first is not None and now - self.since > self.cfg.settle

    def window(self, now, gaps, floor):
        i_now = int(now / self.BUCKET)
        max_gap = max([now - self.last] + [b[3] for b in self.buckets if b[0] >= i_now - 29])
        look = min(120.0, max(floor, gaps * max_gap))
        return [b for b in self.buckets if b[0] >= i_now - math.ceil(look / self.BUCKET) + 1]

    def observe(self, now, d):
        i_now = int(now / self.BUCKET)
        if self.first is None:
            self.first = self.last = self.since = now
            self.ref = d
            self.buckets.append([i_now, d, None, 0.0])
            return
        stable = self.stable(now)       # judged before this chunk moves the edge
        gap, self.last = now - self.last, now
        if self.buckets[-1][0] == i_now:
            b = self.buckets[-1]
            b[1], b[3] = min(b[1], d), max(b[3], gap)
            if stable:
                b[2] = d if b[2] is None else max(b[2], d)
        else:
            self.buckets.append([i_now, d, d if stable else None, gap])
        while len(self.buckets) > 1 and self.buckets[0][0] < i_now - 59:
            self.buckets.popleft()
        e = self.estimate(now)[0]
        if self.ref - e > self.cfg.stab_tol:
            self.ref, self.since = e, now
        elif e > self.ref:
            self.ref = e

    def estimate(self, now):
        """(fastest, slowest, fastest of the recent look-back). The slowest is None while no
        chunk was seen with a stable edge, the recent one when nothing arrived lately."""
        inside = self.window(now, 4, self.lookback) or [self.buckets[-1]]
        slow = [b[2] for b in inside if b[2] is not None]
        recent = self.window(now, 2, 2.0)
        return (min(b[1] for b in inside), max(slow) if slow else None,
                min(b[1] for b in recent) if recent else None)


def simulate_today(cfg, chunks, t_end, reference=None):
    """Model of the current implementation (state.rs, decision_*.rs, buffer.rs) with one track.
    `master` has one more estimator that survives a reset, and keeps the anchor over it."""
    names = sorted({c[2] for c in chunks})
    if len(names) != 1:
        return None                 # two tracks need Mode::Independent, which is not modeled
    master = TODAY["variant"] == "master"
    tr = Track(cfg, names[0])
    res = Result(tracks={tr.name: tr})
    own = TodayEstimator(cfg, TODAY["lookback"])
    shared = TodayEstimator(cfg, TODAY["lookback"]) if master else own
    T = target = None               # the anchor as a delivery delay; None without a mode
    started, why = False, None
    last_written = last_rel = None
    rel_until = -math.inf           # where the released content ends, on the output timeline
    recent, gaps = deque(), deque()     # (arrival, d) and (arrival, gap), for the trace only
    i, now, dt = 0, 0.0, 0.01

    def log(text):
        res.log.append((round(now, 2), text))

    def out(p, anchor):
        nonlocal rel_until
        play = p + anchor
        rel_until = max(rel_until, play)
        if play > now:
            tr.schedule(play)
        else:
            tr.dropped += cfg.frame

    def reset(reason, keep_anchor):
        nonlocal T, target, started, why, last_rel
        anchor = T if T is not None else now + cfg.start - last_written[0]
        while tr.buf:
            out(tr.buf.pop(0)[0], anchor)
        own.reset()
        log(f"reset: {reason}")
        started, why = False, reason
        if not keep_anchor:
            T = target = last_rel = None

    while now < t_end:
        while i < len(chunks) and chunks[i][0] <= now:
            a, p, _ = chunks[i]
            i += 1
            if last_written is not None:
                lp, la = last_written
                if p > lp + (a - la) + 10.0 or p + 10.0 < lp:
                    reset("pts discontinuity", False)
                    shared.reset()
                gaps.append((a, a - la))
            last_written = (p, a)
            own.observe(a, a - p)
            if master:
                shared.observe(a, a - p)
            tr.buf.append((p, a))
            recent.append((a, a - p))
        if last_written is None:
            now += dt
            continue
        if started:
            e, _, e_recent = own.estimate(now)
            if now - last_written[1] >= 1.0 and not tr.buf and rel_until + 1.0 <= now:
                reset("ran out of content", master)
            elif e_recent is not None and e_recent - e >= cfg.stale:
                reset("stale estimate", master)
        if not started and own.first is not None and (
                own.stable(now) and shared.stable(now)
                or now - own.first >= (cfg.max_wait or 2 * cfg.start)):
            e, d, _ = shared.estimate(now)
            if T is None:
                T = target = max(e + cfg.start, (e if d is None else d) + cfg.lower)
            started = True
            if res.first_play is None:
                res.first_play = now
            elif why == "ran out of content":
                res.free_jumps += 1
            else:
                res.jumps += 1
            log(f"start: buffer {T - e:.2f}s")
        if T is not None and shared.first is not None:
            e, d, _ = shared.estimate(now)
            lo = (e if d is None else d) + cfg.lower
            if (d is not None and T < lo - 1e-9) or T > max(e + cfg.upper, lo) + 1e-9:
                if abs(max(e + cfg.start, lo) - target) > 0.05:
                    log(f"target {max(e + cfg.start, lo) - T:+.2f}s from buffer {T - e:.2f}s")
                target = max(e + cfg.start, lo)
            if master and not started:
                T = target          # nothing applies the anchor, so it moves at once
        T_was = T
        while started and tr.buf and tr.buf[0][0] + T <= now + 5.0:
            p = tr.buf.pop(0)[0]
            step = 0.0 if last_rel is None else max(0.0, p - last_rel)
            last_rel = p if last_rel is None else max(last_rel, p)
            if T != target:
                ratio = abs(T - target) / cfg.start
                rate = 0.01 + (0.02 * min(1.0, ratio / 3) if T > target
                               else 0.03 * min(1.0, ratio))
                T += math.copysign(min(abs(target - T), step * rate), target - T)
            out(p, T)
        if T != T_was:
            res.slew_time += dt
        if tr.on_air(now) is False:
            res.interrupted_time += dt
            tr.dry_time += dt
        # the trace is the buffer of the fastest packets of the last arrival cycle
        while gaps and gaps[0][0] < now - 60.0:
            gaps.popleft()
        cycle = 1.5 * max([now - last_written[1]] + [g for _, g in gaps])
        while len(recent) > 1 and recent[0][0] < now - max(2.0, cycle):
            recent.popleft()
        res.trace.append((now, math.nan if T is None else T - min(d for _, d in recent)))
        now += dt
    return res


# ---------------------------------------------------------------------------------- generators

def lcg(seed):
    state = seed

    def rnd(*_):
        nonlocal state
        state = (state * 1103515245 + 12345) % (1 << 31)
        return state / (1 << 31)
    return rnd


def cont(t0, t1, pts0, jitter=None, frame=0.02, name="audio", rate=1.0):
    """Real-time delivery from t0 to t1 of content starting at pts0."""
    out, t, prev = [], t0, -math.inf
    while t < t1:
        prev = max(prev, t + (jitter(t) if jitter else 0.0))    # the transport keeps the order
        out.append((prev, pts0 + (t - t0) * rate, name))
        t += frame
    return out


def spikes(seed, height, every, t_from=0.0, t_to=math.inf, base=0.03):
    """Network delay: about once per `every` delivery stops for up to `height`, then the backlog
    arrives at once. `base` is the jitter in between."""
    rnd = lcg(seed)
    state = {"next": t_from + every * rnd(), "until": -1.0}

    def delay(t):
        if not t_from <= t < t_to:
            return base * rnd()
        while t >= state["next"]:
            state["until"] = state["next"] + height * rnd()
            state["next"] += 2 * every * rnd()
        return max(base * rnd(), state["until"] - t)
    return delay


def steps(seed, height, high, period, t_from, t_to, base=0.02):
    """Network delay that steps up by `height` for `high` seconds once per `period`."""
    rnd = lcg(seed)

    def delay(t):
        j = base * rnd()
        if t_from <= t < t_to and ((t - t_from) % period) < high:
            return height + j
        return j
    return delay


def burst(t0, pts0, length, speed, frame=0.02, name="audio"):
    """`length` seconds of content delivered `speed` times faster than real time."""
    out, p = [], 0.0
    while p < length:
        out.append((t0 + p / speed, pts0 + p, name))
        p += frame
    return out


def hiccups(times, stall, t_end, jitter, speed=10.0):
    """Real-time delivery; at each of `times` nothing arrives for `stall`, then the backlog."""
    out, t, back = [], 0.0, stall * speed / (speed - 1)
    for h in times:
        out += cont(t, h, t, jitter) + burst(h + stall, h, back, speed)
        t = h + back
    return out + cont(t, t_end, t, jitter)


def hls(seg, n, t1=None, speed=10.0, first=3, jitter=None, start_seg=0):
    """Initial batch of `first` segments at t=0, the next segment at t1, then one per `seg`."""
    t1 = seg if t1 is None else t1
    out = burst(0.0, start_seg * seg, first * seg, speed)
    for k in range(n):
        t = t1 + k * seg + (jitter(k) if jitter else 0.0)
        out += burst(t, (start_seg + first + k) * seg, seg, speed)
    return out


def report(name, cfg, chunks, t_end, at=(), reference=None, events=12):
    r = (simulate_today if TODAY else simulate)(cfg, sorted(chunks), t_end, reference)
    print(f"== {name}")
    if r is None:
        print("   not modeled: two tracks")
        return r
    print(f"   first frame {r.first_play:.2f}s | jumps {r.jumps}, re-anchored while interrupted "
          f"{r.free_jumps}, slewing {r.slew_time:.1f}s | nothing playing {r.interrupted_time:.1f}s")
    for tr in r.tracks.values():
        late = f", shown late {tr.shown_late:.1f}s" if tr.shown_late else ""
        print(f"   {tr.name}: played {tr.played:.1f}s, dropped {tr.dropped:.1f}s{late}, "
              f"dry {tr.dry_time:.1f}s")
    for t, text in r.log[:events]:
        print(f"   {t:8.2f}  {text}")
    if len(r.log) > events:
        print(f"   ... {len(r.log) - events} more events")
    if at:
        pts = []
        for t in at:
            k = min(range(len(r.trace)), key=lambda k: abs(r.trace[k][0] - t))
            pts.append(f"{t:g}s: {r.trace[k][1]:.2f}")
        print("   buffer of the fastest packets at " + ", ".join(pts))
    return r


RTMP = dict(lower=1.0, start=2.0, upper=5.0, settle=0.5, tol=0.1, w_base=1.0, start_timeout=5.0,
            stitch=True)
HLS = dict(lower=2.0, start=10.0, upper=20.0, settle=1.0, tol=0.25, w_base=2.0,
           start_timeout=30.0, max_slew_time=60.0, segmented=True, stitch=True,
           stab_tol=0.25, stale=10.0, max_wait=10.0)
MOQ_LL = dict(lower=0.1, start=0.25, upper=0.5, settle=0.5, tol=0.025, w_base=1.0,
              start_timeout=2.0, stale=0.5)
MOQ = dict(lower=1.0, start=2.0, upper=5.0, stale=0.5)


def main(which):
    def want(tag):
        return not which or tag in which

    if want("rtmp"):
        rnd = lcg(1)
        jit = lambda t: 0.05 * rnd()
        head = burst(0.0, 0.0, 3.0, 10.0) + cont(0.3, 30, 3.0, jit)
        report("RTMP startup burst of 3s, jitter 0-50ms (G1, R2)", Cfg(**RTMP),
               burst(0.0, 0.0, 3.0, 10.0) + cont(0.3, 120, 3.0, jit), 120, at=(5, 60, 119))
        report("RTMP 5s stall, backlog flushed at 5x (G7c, R3)", Cfg(**RTMP),
               head + burst(35.0, 32.7, 6.25, 5.0) + cont(36.25, 120, 38.95, jit), 120,
               at=(29, 37, 45, 119))
        report("RTMP 5s stall, content continues where it stopped (G7a)", Cfg(**RTMP),
               head + cont(35.0, 120, 32.7, jit), 120, at=(29, 37, 45, 119))
        report("RTMP 5s stall, content skipped (G7b)", Cfg(**RTMP),
               head + cont(35.0, 120, 37.7, jit), 120, at=(29, 37, 119))
        report("RTMP 1.5s gap covered by the buffer, then a burst (G13c)", Cfg(**RTMP),
               head + burst(31.5, 32.7, 1.875, 5.0) + cont(31.875, 120, 34.575, jit), 120,
               at=(29, 33, 119))
        report("RTMP 0.6s gap, content continues (G13a, inside the limits)", Cfg(**RTMP),
               head + cont(30.6, 120, 32.7, jit), 120, at=(29, 33, 119))
        report("RTMP 1.5s gap, content continues (G13a, under the lower limit)", Cfg(**RTMP),
               head + cont(31.5, 120, 32.7, jit), 120, at=(29, 33, 60, 119))
        per, t = burst(0.0, 0.0, 3.0, 10.0) + cont(0.3, 20, 3.0, jit), 20.0
        while t < 200:      # every 20s: 2s without delivery, then the backlog at 5x
            per += burst(t + 2.0, t + 2.7, 2.5, 5.0) + cont(t + 2.5, t + 20, t + 5.2, jit)
            t += 20.0
        report("RTMP 2s stall with a backlog burst every 20s, ends at 200s (periodic, G4)",
               Cfg(**RTMP), per + cont(200, 330, 202.7, jit), 330, at=(15, 30, 50, 190, 250, 329),
               events=16)
        report("RTMP 2s stall with a backlog burst every 50s, ends at 230s (LT4)", Cfg(**RTMP),
               hiccups((30, 80, 130, 180, 230), 2.0, 400, jit, speed=5.0), 400,
               at=(25, 70, 100, 150, 220, 290, 320, 399), events=16)
        report("RTMP 2s stall with a backlog burst every 90s, ends at 300s (LT3: too far apart)",
               Cfg(**RTMP),
               hiccups((30, 120, 210, 300), 2.0, 520, jit, speed=5.0), 520,
               at=(25, 100, 130, 200, 290, 400, 470, 519), events=16)

    if want("hls"):
        for t1 in (6.0, 3.0):
            report(f"HLS 6s segments, next segment {t1:g}s after start (H4)", Cfg(**HLS),
                   hls(6, 30, t1), 180, at=(8, 60, 179))
        rnd = lcg(7)
        report("HLS 6s segments, segment arrival jitter 0-1.5s (H5)", Cfg(**HLS),
               hls(6, 40, jitter=lambda k: 1.5 * rnd()), 240, at=(10, 60, 120, 239))
        for t1 in (20.0, 8.0):
            report(f"HLS 20s segments, next segment {t1:g}s after start (H2)", Cfg(**HLS),
                   hls(20, 12, t1), 240, at=(15, 45, 120, 239))
        report("HLS 20s segments, no 'segmented' hint (H2)", Cfg(**dict(HLS, segmented=False)),
               hls(20, 12), 240, at=(15, 45, 120, 239))
        report("HLS 2s segments, buffer 2s/3s/6s (H10)",
               Cfg(**dict(HLS, lower=2.0, start=3.0, upper=6.0)), hls(2, 60, 1.5), 120,
               at=(5, 30, 119))
        st = burst(0.0, 0.0, 60, 10.0)
        for k in range(3):
            st += burst((k + 1) * 20.0, 60 + k * 20, 20, 10.0)
        st += burst(120.0, 120.0, 60, 10.0)         # nothing for 60s, then three segments at once
        for k in range(8):
            st += burst(140.0 + k * 20, 180 + k * 20, 20, 10.0)
        report("HLS 20s segments, origin stalls, 3 segments at once (H7, G7c)", Cfg(**HLS), st,
               300, at=(100, 125, 150, 299), events=16)
        pa = burst(0.0, 0.0, 18, 10.0)
        for k in range(8):
            pa += burst((k + 1) * 6.0, 18 + k * 6, 6, 10.0)
        for k in range(20):                          # source paused for 60s, content continues
            pa += burst(108.0 + k * 6, 66 + k * 6, 6, 10.0)
        report("HLS 6s segments, source paused for 60s, content continues (H7, G7a)", Cfg(**HLS),
               pa, 220, at=(40, 110, 130, 219))
        ch = burst(0.0, 0.0, 18, 10.0)
        for k in range(10):
            ch += burst((k + 1) * 6.0, 18 + k * 6, 6, 10.0)
        for k in range(8):                           # segments become 20s long
            ch += burst(80.0 + k * 20, 78 + k * 20, 20, 10.0)
        report("HLS segments change from 6s to 20s (H6)", Cfg(**HLS), ch, 240,
               at=(50, 85, 110, 239))
        dj = burst(0.0, 0.0, 18, 10.0)
        for k in range(10):
            dj += burst((k + 1) * 6.0, 18 + k * 6, 6, 10.0)
        for k in range(15):                          # pts restart from 5000
            dj += burst(66.0 + k * 6, 5000 + k * 6, 6, 10.0)
        report("HLS discontinuity, pts jump +4900s at a segment boundary (H8)", Cfg(**HLS), dj,
               150, at=(50, 70, 149))
        report("HLS same discontinuity, stitching off", Cfg(**dict(HLS, stitch=False)), dj, 150,
               at=(50, 70, 80, 149))

    if want("moq"):
        rnd = lcg(3)
        j50 = lambda t: 0.05 * rnd()
        for mode in (False, True):
            tag = "upper limit enforced" if mode else "upper limit extends"
            report(f"MoQ 100/250/500ms, {tag}: delays up to 400ms from 40s on (M4, G15)",
                   Cfg(enforce=mode, **MOQ_LL), cont(0, 120, 0, spikes(21, 0.4, 1.0, 40)), 120,
                   at=(30, 45, 60, 119))
            report(f"MoQ 100/250/500ms, {tag}: delays up to 400ms from 40s to 80s (M3)",
                   Cfg(enforce=mode, **MOQ_LL), cont(0, 160, 0, spikes(21, 0.4, 1.0, 40, 80)),
                   160, at=(30, 60, 85, 95, 159))
        report("MoQ 1s/2s/5s: delays up to 400ms from 40s to 80s (inside the limits, G2)",
               Cfg(**MOQ), cont(0, 160, 0, spikes(21, 0.4, 1.0, 40, 80)), 160, at=(30, 60, 159))
        report("MoQ 1s/2s/5s: delays up to 1.8s from 40s to 80s (M4 then M3)",
               Cfg(**MOQ), cont(0, 200, 0, spikes(21, 1.8, 3.0, 40, 80)), 200,
               at=(30, 60, 79, 100, 199))
        report("MoQ 100/250/500ms: one 1s hiccup, backlog at 10x (M5)", Cfg(**MOQ_LL),
               cont(0, 40, 0, j50) + burst(41.0, 40.0, 1.11, 10.0) + cont(41.111, 120, 41.11, j50),
               120, at=(30, 42, 60, 119))
        per, t = [], 0.0
        while t < 150:
            per += cont(t, t + 14.4, t, j50) + burst(t + 15.0, t + 14.4, 0.667, 10.0)
            t += 15.0667
        report("MoQ 100/250/500ms, extends: 0.6s hiccup every 15s, ends at 150s", Cfg(**MOQ_LL),
               per + cont(t, 260, t, j50), 260, at=(10, 20, 40, 149, 200, 259))
        report("MoQ 100/250/500ms, enforced: 0.6s hiccup every 15s",
               Cfg(enforce=True, **MOQ_LL), per, 150, at=(10, 40, 149))
        groups = [g + k for g in (20, 60, 100, 140, 180) for k in (0, 1, 2)]
        report("MoQ 100/250/500ms, extends: three 0.6s hiccups 1s apart, every 40s (LT4)",
               Cfg(**MOQ_LL), hiccups(groups, 0.6, 300, j50), 300,
               at=(15, 24, 55, 64, 95, 135, 175, 250, 299), events=20)

    if want("av"):
        rnd = lcg(11)
        j30 = lambda t: 0.03 * rnd()
        audio = cont(0, 150, 0, j30, name="audio")
        # video: on time, from 30s to 60s delivered at 70% of real time, then the backlog at 3x
        video = cont(0, 30, 0, j30, name="video")
        video += cont(30, 60, 30, j30, name="video", rate=0.7)
        video += burst(60.0, 51.0, 13.5, 3.0, name="video")
        video += cont(64.5, 150, 64.5, j30, name="video")
        for mode in (True, False):
            tag = "upper limit enforced" if mode else "upper limit extends"
            report(f"MoQ 100/250/500ms, {tag}: video at 70% of real time from 30s to 60s (M1-M3)",
                   Cfg(enforce=mode, **MOQ_LL), audio + video, 150, at=(25, 45, 59, 62, 70, 149),
                   events=14)
        late = cont(0, 100, 0, j30, name="audio") + \
            cont(0, 100, -0.8, j30, name="video")      # video always 0.8s later than audio
        report("MoQ 100/250/500ms, enforced: audio received 0.8s earlier than video (M2)",
               Cfg(enforce=True, **MOQ_LL), late, 100, at=(10, 99))
        report("MoQ 1s/2s/5s, extends: audio received 0.8s earlier than video",
               Cfg(lower=1.0, start=2.0, upper=5.0), late, 100, at=(10, 99))
        pause = cont(0, 100, 0, j30, name="video") + cont(0, 30, 0, j30, name="audio") + \
            cont(60, 100, 60, j30, name="audio")       # audio muted from 30s to 60s
        report("RTMP audio pauses for 30s while video continues (G16)", Cfg(**RTMP), pause, 100,
               at=(29, 45, 61, 99))

    if want("misc"):
        rnd = lcg(5)
        jit = lambda t: 0.05 * rnd()
        for name, dj in (("+30s", 30.0), ("-30s", -30.0), ("+3s", 3.0), ("-3s", -3.0),
                         ("-0.5s", -0.5)):
            ch = cont(0, 30, 0) + cont(30, 120, 30 + dj)
            report(f"RTMP pts jump {name} (G6)", Cfg(**RTMP), ch, 120, at=(29, 31, 35, 119))
        for name, dj in (("+30s", 30.0), ("-30s", -30.0)):
            ch = cont(0, 30, 0) + cont(30, 120, 30 + dj)
            report(f"MoQ-like (no stitching) pts jump {name}", Cfg(**dict(RTMP, stitch=False)),
                   ch, 120, at=(29, 31, 35, 119))
        for name, k, end in (("0.1% fast", 1.001, 240), ("3% slow", 0.97, 240),
                             ("3% fast", 1.03, 540), ("10% slow", 0.9, 240),
                             ("10% fast", 1.1, 240)):
            report(f"RTMP source clock {name} (G5, G8)", Cfg(**RTMP), cont(0, end, 0, rate=k),
                   end, at=(10, 100, 239, end - 1), events=6)

    if "review" in which:       # scenarios of DESIGN.md section 14, items 1, 2, 5 and 6
        at = (15, 30, 62, 94, 99, 110, 130, 169)
        for mode in (True, False):
            tag = "enforced" if mode else "extends"
            report(f"MoQ 100/250/500ms, {tag}: delay +300ms for 3s every 8s, 20s to 100s (1)",
                   Cfg(enforce=mode, **MOQ_LL), cont(0, 170, 0, steps(3, 0.3, 3, 8, 20, 100)),
                   170, at=at, events=40)
        report("RTMP: delay +1.5s for 4s every 12s, 20s to 104s (1)", Cfg(**RTMP),
               cont(0, 170, 0, steps(5, 1.5, 4, 12, 20, 104)), 170, at=at, events=40)
        for name, cfg, step in (("MoQ 100/250/500ms", MOQ_LL, 0.5), ("RTMP", RTMP, 3.0)):
            sparse = cont(0, 200, 0, lambda t, s=step: 0.01 + (s if t >= 60 else 0.0), frame=3.0,
                          name="video")
            report(f"{name}, video only, a frame every 3s: delay +{step:g}s from 60s on (2)",
                   Cfg(**cfg), sparse, 200, at=(30, 59, 70, 120, 199))
        report("MoQ 100/250/500ms: source 8% fast, the edge never settles (5)", Cfg(**MOQ_LL),
               cont(0, 90, 0, rate=1.08), 90, at=(3, 10, 30, 60, 89))
        st, free = burst(0.0, 0.0, 18, 10.0), 1.8
        for k in range(30):     # 6s segments, one after another; the ninth stalls 8s half way
            t, p = max(6.0 + k * 6, free), 18 + k * 6
            if k == 8:
                st += burst(t, p, 3, 10.0) + burst(t + 8.3, p + 3, 3, 10.0)
                free = t + 8.6
            else:
                st += burst(t, p, 6, 10.0)
                free = t + 0.6
        report("HLS 6s segments, one download stalls 8s in the middle, at 54s (6)", Cfg(**HLS),
               st, 186, at=(50, 60, 64, 70, 90, 120, 185))


if __name__ == "__main__":
    args = sys.argv[1:]
    for arg in [a for a in args if a.startswith(("master", "branch"))]:
        # e.g. `master`, `master10`: the current implementation, with its window at 20s or 10s
        TODAY = {"variant": arg[:6], "lookback": float(arg[6:] or 20)}
        args.remove(arg)
    main(args)
