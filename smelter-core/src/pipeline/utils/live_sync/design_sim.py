"""Simulation of the live sync rules proposed in DESIGN.md (behavior only, no media).

Everything is in seconds. A chunk is (arrival, pts, track). Delivery delay d = arrival - pts.
Run: python3 sim.py [rtmp] [hls] [moq] [av] [misc]
"""
import bisect
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


class Edge:
    """Fastest delivery. Falls at once, rises only when the whole window is later and flat."""

    def __init__(self, cfg):
        self.cfg = cfg
        self.samples = deque()      # (arrival, d) inside the window
        self.lateness = 0.0         # regular lateness of the track, set by the caller
        self.value = None
        self.last_arrival = None
        self.fed_since = None
        self.mark = None
        self.last_fall = None
        self.pending = []
        self.rises = []             # (delta, window start)

    def window(self, now):
        """Twice the regular lateness of the track: with bursty delivery that is two cycles."""
        return min(self.cfg.w_max, max(self.cfg.w_base, 2 * self.lateness))

    def observe(self, a, d):
        cfg = self.cfg
        if self.last_arrival is not None:
            gap = a - self.last_arrival
            if gap > 0.75 * self.window(a):
                self.fed_since = a
        else:
            self.fed_since = a
        self.last_arrival = a
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
        if self.value < self.mark - cfg.tol:
            self.mark, self.last_fall = self.value, a

    def candidate(self, now):
        w = self.window(now)
        while self.samples and self.samples[0][0] < now - w:
            self.samples.popleft()
        if not self.samples:
            return None
        c = min(d for _, d in self.samples)
        t = min(t for t, d in self.samples if d <= c + 1e-9)
        return c, t

    def tick(self, now):
        cand = self.candidate(now)
        if cand is None or self.value is None:
            return
        c, t = cand
        w = self.window(now)
        if c > self.value + self.cfg.tol and now - self.fed_since >= w and now - t >= self.cfg.settle:
            self.rises.append((c - self.value, now - w - 0.05))
            self.value, self.mark = c, c

    def settled(self, now):
        return self.last_fall is not None and now - self.last_fall >= self.cfg.settle


class Depth:
    """Lateness that repeats: reached twice -> regular; kept for twice its period."""

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
        else:
            b[0], b[1] = max(b[0], d), min(b[1], d)
        if self.cfg.segmented and gap is not None and gap >= self.cfg.gap_min:
            self.first_sight = (a, d, gap)
        self.cache = (None, None)

    def shift(self, delta, before):
        """The edge rose by `delta`: what was measured before the window moves with it."""
        limit = int(before / self.cfg.bucket)
        for i, b in self.buckets.items():
            if i < limit:
                b[0] += delta
                b[1] += delta
        if self.first_sight and self.first_sight[0] < before:
            t, d, g = self.first_sight
            self.first_sight = (t, d + delta, g)
        self.cache = (None, None)

    def shift_all(self, delta):
        self.shift(delta, math.inf)

    def slow(self, now, edge):
        cfg = self.cfg
        i_now = int(now / cfg.bucket)
        key = (i_now, round(edge, 4))
        if self.cache[0] == key:
            return self.cache[1]
        for i in [i for i in self.buckets if i < i_now - int(cfg.mem / cfg.bucket)]:
            del self.buckets[i]
        idx = sorted(self.buckets)
        best = edge
        for level in sorted({self.buckets[i][0] for i in idx if self.buckets[i][0] > edge},
                            reverse=True):
            h = max(0.01, 0.1 * (level - edge))
            j = next((i for i in reversed(idx) if self.buckets[i][0] >= level), None)
            m = next((i for i in reversed(idx) if i < j and self.buckets[i][1] < level - h), None)
            if m is None:
                continue
            i0 = next((i for i in reversed(idx) if i < m and self.buckets[i][0] >= level), None)
            if i0 is None:
                continue
            if (i_now - j) * cfg.bucket <= max(cfg.keep_min, 2 * (j - i0) * cfg.bucket):
                best = level
                break
        if self.first_sight:
            t, d, gap = self.first_sight
            if now - t <= max(cfg.keep_min, 2 * (d - edge)):
                best = max(best, d)
        self.cache = (key, best)
        return best


class Track:
    def __init__(self, cfg, name):
        self.cfg, self.name = cfg, name
        self.edge, self.depth = Edge(cfg), Depth(cfg)
        self.buf = []               # (pts, arrival), ordered by pts
        self.played_until = -math.inf
        self.counts = False         # measured once: its burst has ended
        self.first_arrival = None
        self.rises_seen = 0
        self.offset = 0.0           # stage 0
        self.last_raw = None        # (raw pts, arrival, stitched pts)
        self.played = self.dropped = self.shown_late = self.dry_time = 0.0


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
    T = None
    moving = False
    i, now, dt = 0, 0.0, 0.01

    def log(text):
        res.log.append((round(now, 2), text))

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
                tr.edge.lateness = max(0.0, tr.depth.slow(now, tr.edge.value) - tr.edge.value)
            tr.edge.tick(now)
            while tr.rises_seen < len(tr.edge.rises):
                delta, before = tr.edge.rises[tr.rises_seen]
                tr.depth.shift(delta, before)
                tr.rises_seen += 1
                log(f"{tr.name}: edge +{delta:.2f}s")
            if not tr.counts and T is not None and tr.edge.settled(now):
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
            T = E + cfg.start
            res.first_play = now
            log(f"start: buffer {cfg.start:.2f}s" + ("" if ready else " (timeout)"))

        # ---- stage 2: combine and decide
        active = [t for t in tracks.values()
                  if t.counts and now - t.edge.last_arrival <= cfg.idle] or \
                 [t for t in tracks.values() if t.counts]
        E = max(t.edge.value for t in active)
        D = max(max(t.depth.slow(now, t.edge.value), t.edge.value) for t in active)
        P = D - E
        Bs = max(cfg.start, P + cfg.lower)
        T_star, T_lo, T_hi = E + Bs, D + cfg.lower, E + cfg.upper + (Bs - cfg.start)
        cand = max((t.edge.candidate(now) or (t.edge.value, 0))[0] for t in active)
        T_cand = cand + Bs
        ref = tracks.get(reference)
        D_must = D                  # packets that must not be lost: a jump is worth it for them
        if cfg.enforce and ref is not None and ref.counts:
            D_must = max(ref.depth.slow(now, ref.edge.value), ref.edge.value)
            cap = ref.edge.value + cfg.upper
            T_star, T_lo, T_hi = min(T_star, cap), min(T_lo, cap), min(T_hi, cap)
            T_cand = min(T_cand, (ref.edge.candidate(now) or (ref.edge.value, 0))[0] + cfg.upper)
        interrupted = all(t.played_until <= now for t in tracks.values())
        if interrupted:
            res.interrupted_time += dt
        err = T_star - T
        if interrupted:
            if T < T_lo - cfg.tol or T > T_hi + cfg.tol:
                T = T_star
                res.free_jumps += 1
                moving = False
                log(f"interrupted: re-anchored {err:+.2f}s -> buffer {T - E:.2f}s")
        elif moving or T < T_lo - cfg.tol or T > T_hi + cfg.tol:
            jump_thr = cfg.rate_max * cfg.max_slew_time
            if T < D_must - cfg.tol or T - T_hi > jump_thr:
                why = "regular packets late" if T < D_must - cfg.tol else "too far to slew"
                T = T_star
                res.jumps += 1
                moving = False
                log(f"jump {err:+.2f}s -> buffer {T - E:.2f}s ({why})")
            else:
                if not moving:
                    log(f"slew {err:+.2f}s from buffer {T - E:.2f}s")
                moving = abs(err) > 0.002
                rate = min(cfg.rate_max, max(cfg.rate_min, abs(err) / cfg.rate_tau))
                T += math.copysign(min(abs(err), rate * dt), err)
                res.slew_time += dt

        # ---- stage 3: release
        for tr in tracks.values():
            if tr.played_until <= now:
                tr.dry_time += dt
            while tr.buf:
                p, a = tr.buf[0]
                play = p + T
                if play - now > cfg.ahead:
                    break
                if play >= now:
                    del tr.buf[0]
                    tr.played += cfg.frame
                    tr.played_until = max(tr.played_until, play + cfg.frame)
                    continue
                if interrupted and T_cand > T + cfg.tol and p + T_cand >= now:
                    break           # kept: a restart at the candidate edge would still play it
                del tr.buf[0]
                if cfg.enforce and tr.name != reference:
                    tr.shown_late += cfg.frame
                else:
                    tr.dropped += cfg.frame
        res.trace.append((now, T - E))
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


def burst(t0, pts0, length, speed, frame=0.02, name="audio"):
    """`length` seconds of content delivered `speed` times faster than real time."""
    out, p = [], 0.0
    while p < length:
        out.append((t0 + p / speed, pts0 + p, name))
        p += frame
    return out


def hls(seg, n, t1=None, speed=10.0, first=3, jitter=None, start_seg=0):
    """Initial batch of `first` segments at t=0, the next segment at t1, then one per `seg`."""
    t1 = seg if t1 is None else t1
    out = burst(0.0, start_seg * seg, first * seg, speed)
    for k in range(n):
        t = t1 + k * seg + (jitter(k) if jitter else 0.0)
        out += burst(t, (start_seg + first + k) * seg, seg, speed)
    return out


def report(name, cfg, chunks, t_end, at=(), reference=None, events=12):
    r = simulate(cfg, sorted(chunks), t_end, reference)
    print(f"== {name}")
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
           start_timeout=30.0, max_slew_time=60.0, segmented=True, stitch=True)
MOQ_LL = dict(lower=0.1, start=0.25, upper=0.5, settle=0.5, tol=0.025, w_base=1.0,
              start_timeout=2.0)


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
               Cfg(lower=1.0, start=2.0, upper=5.0), cont(0, 160, 0, spikes(21, 0.4, 1.0, 40, 80)),
               160, at=(30, 60, 159))
        report("MoQ 1s/2s/5s: delays up to 1.8s from 40s to 80s (M4 then M3)",
               Cfg(lower=1.0, start=2.0, upper=5.0), cont(0, 200, 0, spikes(21, 1.8, 3.0, 40, 80)),
               200, at=(30, 60, 79, 100, 199))
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
        for name, k in (("0.1% fast", 1.001), ("3% slow", 0.97), ("3% fast", 1.03),
                        ("10% slow", 0.9), ("10% fast", 1.1)):
            report(f"RTMP source clock {name} (G5, G8)", Cfg(**RTMP), cont(0, 240, 0, rate=k),
                   240, at=(10, 100, 239), events=6)


if __name__ == "__main__":
    main(sys.argv[1:])
