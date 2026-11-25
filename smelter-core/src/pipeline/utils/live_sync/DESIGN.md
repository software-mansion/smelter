# Live sync: design proposal

A proposal for sections 3-5 of `REDESIGN.md`, written against its sections 1 and 2 as of
2026-11-24. Everything in this file was proposed by Claude and nothing is confirmed, so items are
not marked `(?)` one by one. `[today]` marks the current implementation. Scenario ids (G1, H2,
...) are the ones of `REDESIGN.md`; rule ids (ED2, DC3, ...) are defined in section 4.

Revised on 2026-11-25 after a review by another session. The review is section 14, and section
15 says where each of its findings went.

1. The idea
2. Model: what is measured, what is controlled, the band
3. Structure: four stages
4. Rules, stage by stage
5. Interface: public option, input-facing API, buffer contract, stats, constants
6. Detections that can misread (section 3 of `REDESIGN.md`)
7. Hints (section 4 of `REDESIGN.md`)
8. Scenarios, one by one
9. What is given up, what differs per protocol, requirements that were not listed
10. Choices inside the design, other designs, and today's implementation with other parameters
11. Where the rules come from
12. What was checked in a simulation
13. Decisions to review, and what is left open
14. Review findings
15. Where the review findings went

## 1. The idea

Every chunk has an arrival time and a pts. Their difference, the **delivery delay**, is the only
thing live sync looks at. Two numbers are measured from it, per track:

- the **edge**: the delivery delay of the fastest delivery. It says where live is;
- the **regular lateness**: how much later than the edge content arrives regularly (jitter, the
  length of an HLS segment, a stall that keeps coming back).

The playback timeline is compared with these two numbers through the limits the user configured.
Everything else follows from five rules:

1. **Two measurements, two memories.** Where live is, is believed quickly: faster delivery at
   once, slower delivery once it has stayed away from the old edge for longer than it regularly
   does. How late content regularly arrives is believed only when it repeats, and it is kept
   for as long as it keeps repeating.
2. **Inside the band nothing moves.** The band is the user's limits applied to the two numbers:
   the regular slowest packets have at least the lower limit of buffer, the fastest ones at most
   the upper limit.
   - One exception: latency that live sync added itself is taken back when its reason is gone.
3. **Outside the band the buffer goes back to the target**, not just back inside.
4. **A re-anchor is as abrupt as what the viewer already sees.** Nothing is playing: jump.
   Content is being lost anyway: jump. Otherwise slew. Only a buffer that is too large by more
   than a slew can remove in reasonable time is cut with a jump.
5. **Late is late.** The playback timeline never follows a single late packet or a burst.
   Content that misses its time is not played, and it can change the timeline only through
   rule 1.

Start, stall, discontinuity and track changes are not separate mechanisms. Each of them is these
rules applied in a state where nothing is playing, or to a track that is not counted yet.

```
 delivery delay
 (arrival - pts)

 T    ------------------------------------------------------  playback timeline: above it = late
            ^ at least `lower`                 ^
 E+L  - - - x - - - - - - x - - - - - - x - - -|- - - - - -   regular lateness: comes back
            |\            |\      o     |\     | B: at most
            | \           | \     |\    | \    | `upper`      o = seen once: ignored
 E    ======+  \==========+  \====+ \===+  \===v==========    edge: the fastest delivery
                                                     time ->
```

## 2. Model

### 2.1 What is measured

- **Delivery delay** of a chunk: `d = arrival - pts`, on the queue clock. Its absolute value
  means nothing (the pts base is arbitrary); only differences are used.
- **Edge** `E` of a track: the delivery delay of the fastest delivery (4.2). Content can't
  arrive before it exists, so the smallest delay ever seen is where live is, until the timeline
  of the stream itself moves (drift, a pause of the source, a pts jump, a changed network path).
- **Regular lateness** `L` of a track: how far above the edge the delivery delay gets
  regularly (4.2). One late packet or one burst is not regular.
  - With it, how long it lasts, `A`: how long delivery regularly stays away from the edge. It
    is used only to decide how long slower delivery has to last before the edge follows it.

For the input as a whole (tracks on one timeline, 4.3):

- `E` is the latest edge among the tracks: the earliest moment at which the content of every
  track is there.
- `L` is the largest `E + L` of a track, minus the `E` of the input.

### 2.2 What is controlled

- **Playback timeline** `T`: a chunk is presented at `pts + T`. The buffer of a chunk is
  `T - d`.
- **Buffer** `B = T - E`: the buffer of the fastest packets, so how far playback is behind
  live.
- `B - L`: the buffer of the regular slowest packets. When it is negative, content is late
  regularly.

### 2.3 Options

Three durations and one choice, resolved by the input from the public `buffer` option (5.1):

- `lower`: the least buffer the regular slowest packets should have `[today: min]`;
- `start`: the starting buffer, of the fastest packets `[today: desired]`. It is also where
  the buffer is brought back to;
- `upper`: the most buffer the fastest packets should have `[today: max]`;
- what happens when `lower` and `upper` can't both hold:
  - **extend**: `lower` wins. The buffer grows past `upper` as far as the regular lateness
    needs. Nothing is lost.
  - **enforce**: `upper` wins. Playback is never more than `upper` behind the edge of the
    reference track (4.3). What can't arrive in that time is late.

### 2.4 Target and band

```
target   B* = max(start, L + lower)
band     L + lower  <=  B  <=  upper + (B* - start)
```

- While the regular lateness fits under the starting buffer (`L <= start - lower`), the target
  is `start` and the band is `L + lower .. upper`.
- When it does not fit, the window the user configured (`start`, `upper`) moves up as a whole
  by what is missing. The target is then the lower edge of the band: the smallest buffer that
  still gives the regular slowest packets `lower`.
- enforce: the target and both edges of the band are cut off at `upper` above the edge of the
  reference track.
- Every comparison with the band has a tolerance `tol` (5.5).

Examples:

| limits (lower / start / upper) | regular lateness | target | band |
|---|---|---|---|
| RTMP 1s / 2s / 5s | 50ms of jitter | 2s | 1.05s .. 5s |
| RTMP 1s / 2s / 5s | 2s stall every 20s | 3s | 3s .. 6s |
| HLS 2s / 10s / 20s | 6s segments (5.4s) | 10s | 7.4s .. 20s |
| HLS 2s / 10s / 20s | 20s segments (18s) | 20s | 20s .. 30s |
| MoQ 100 / 250 / 500ms, extend | 400ms | 500ms | 500ms .. 750ms |
| MoQ 100 / 250 / 500ms, enforce | 400ms | 500ms | 500ms .. 500ms |
| MoQ 100 / 250 / 500ms, enforce | 700ms | 500ms | 500ms .. 500ms, slowest late |

## 3. Structure: four stages

```
 input --chunk--> 0 STITCH --> 1 MEASURE --> 2 DECIDE --> 3 RELEASE --> decoder --> queue
                  per track    per track     per input    per track
```

| stage | owns | reads | never |
|---|---|---|---|
| 0 Stitch | pts offset of the stream | pts and arrival of consecutive chunks | touches timing |
| 1 Measure | edge and regular lateness per track | arrival and stitched pts | looks at `T` |
| 2 Decide | the playback timeline `T` | stage 1, options, "is anything playing" | looks at chunks |
| 3 Release | the buffer of a track | `T`, the clock | changes `T` |

- Stage 1 does not know what is being played, and stage 2 does not see single chunks. That is
  what keeps a late packet from moving the timeline directly (rule 5).
- All decisions are functions of the sequence of (arrival, pts) and of the clock passed in.
  There is no hidden time source, so every scenario can be replayed in a test without threads.
- It runs on every written chunk and on a tick `[today: 20ms]`.

## 4. Rules, stage by stage

### 4.1 Stage 0: stitch (pts discontinuities)

On for RTMP and HLS, off for MoQ (9.2). With it off, a pts jump is handled by the other stages,
with a visible artifact (G6 in section 8).

For consecutive chunks of one track, in arrival order: `dp` is the difference of their raw pts,
`da` of their arrival, and `J = dp - da` is how far the content moved beyond the time that
passed.

- **ST1. Jump.** `dp < -1s` (the pts went back; no stall can do that), or `J > 10s` (the
  content skipped ahead by much more than the time that passed).
- **ST2. Re-base.** The track that jumps first gets a new pts offset, chosen so that the chunk
  continues the content: its stitched pts is the pts of the previous chunk plus the previous
  frame gap. All later chunks of the track use the offset. The sink gets a `Discontinuity`
  event before the chunk.
- **ST3. Pair.** While the re-base of one track waits for the other track: every chunk of the
  other track is tested, and the first one whose `J` is within `min(1s, |J| / 2)` of the `J`
  of the first track takes over the same offset. It does not compute its own, so the A/V
  relation of the new timeline is kept exactly.
- **ST4. Undo.** If the other track delivers 2s of content without such a chunk, the first
  track goes back to the common offset: it was a gap in one track (R4), not a new timeline.
  While the other track delivers nothing, the re-base just waits.

Why "continues the content" and not "continues the clock": on HLS a discontinuity sits on a
segment boundary, where the new segment follows the old one. After an encoder restart with a
gap (R5) it makes the new content look like a stall where content continues (G7(a)), which the
next stages handle.

After stitching, the delivery delay is continuous over the jump. Stages 1-3 see nothing, and
nothing that was measured is thrown away.

### 4.2 Stage 1: measure (per track)

**Edge.**

- **ED1.** The edge starts as the delivery delay of the first chunk.
- **ED2. Faster: at once.** A chunk with a smaller delivery delay becomes the edge
  immediately.
  - A chunk more than 1s under the edge does not move it by itself. The edge moves when three
    chunks in a row are that far under it, to the smallest of them (one broken timestamp must
    not move anything).
- **ED3. Slower: when delivery stays away from the edge and is not catching up.** Let `c` be
  the smallest delivery delay among the chunks that arrived in the last `W`. The edge moves up
  to `c` when all hold:
  - `c` is above the edge by more than `tol`;
  - both halves of the window have chunks. Time without arrivals proves nothing;
  - the later half has no chunk faster than the earlier half by more than `tol`: delivery is
    not catching up.
  - `W = clamp(2 x A, W_base, 60s)`, where `A` is how long regular lateness lasts (LT9). With
    bursty delivery that is two cycles. A step in delay that keeps coming back has to last
    twice as long as usual before it is taken for a new timeline.
  - On a track that delivered fewer than four chunks in that time (a frame every few
    seconds), the window reaches back to the fourth chunk from the end.
- **ED4. Settled.** The edge is settled when it has not gone down by more than `tol` in total
  during the last `settle`. A burst (delivery faster than real time) keeps the edge falling, so
  "settled" is how the end of a burst is seen.

What ED3 tells apart: after a stall, a backlog is delivered faster than real time, so its delay
keeps falling and ends back at the old edge: nothing moves. A new timeline stays at the new
level, or keeps getting later: after `W` it is the edge. A step in delay that is known to come
back is neither, as long as it lasts no longer than usual: it is lateness (LT7).

**Regular lateness.**

- **LT1. History.** The delivery delays of the last 120s, kept as the smallest and the largest
  per 250ms.
- **LT2. Occasion.** The delay *reaches* a level when a chunk arrives with a delay that is not
  more than a tenth under it (a tenth of the level's height above the edge, at least 10ms):
  delays that close are the same level. Two reaches are separate occasions when delivery was
  clearly faster in between (two tenths under the level). An occasion lasts from the last
  chunk at the edge (within `tol`) before it to the first one after it.
- **LT3. Twice.** A level is regular when the latest two occasions that reached it are not
  more than `repeat_max` (60s, half of the history) apart. An occasion from before the level
  stopped being regular counts like any other.
- **LT4. Keep.** The level stays regular after its latest occasion for twice the longest time
  between two consecutive occasions in the history, at least `keep_min` (10s). Times over
  `repeat_max` are not counted.
  - The longest time, not the latest one: when occasions come in groups, what has to be
    covered is the time between the groups.
  - It is read from the history every time; no keep time is stored. So when two occasions
    were more than 40s apart, the older one leaves the history before twice that time has
    passed, and the level is kept for 120s minus that time. That is never shorter than the
    time between them.
  - Lateness that comes back less often than every 60s is never regular. Each occurrence is
    a one-off.
- **LT5. First sight** (only with the hint "content is delivered in segments", 7). The delay
  of the first chunk after an arrival gap of at least 500ms is regular at once. It stays until
  the next such chunk replaces it, or for twice its own height above the edge (at least
  `keep_min`).
- **LT6. Value.** `L` is what two occasions of the highest regular level both reached (the
  second highest of them), minus the edge; 0 when there is none. One occasion that went a
  little higher than the others does not size the buffer.
- **LT7. It follows the edge up, and a rise that is taken back was lateness.**
  - When ED3 moves the edge up, everything in the history from before the window `W` moves up
    by the same amount. The pattern survives a shift of the timeline.
  - When the edge then comes back at once (ED2) to where it was before the rise (within
    `tol`), that is undone. The time the edge spent higher is then what it was: one occasion
    of lateness of that height. So a step in delay that comes back is known as lateness from
    its second occurrence, and ED3 no longer follows it (its `W` covers the step).
  - When the edge falls for another reason (drift, a pts jump that is not stitched, the next
    HLS segment after the start), the history stays as it is: lateness is measured against
    the edge of now.
- **LT8. It starts when the track counts** (TR1). The initial burst is not lateness.
- **LT9. How long it lasts.** LT2, LT3, LT4 and LT6 applied to how long an occasion lasts
  instead of how high it gets (durations within a tenth of each other, at least 250ms, are the
  same). `A` is the longest regular duration, 0 when there is none.

What the twice rule does: a late packet, a backlog after a stall, a spike, each seen once, never
make the buffer grow. Jitter repeats within a fraction of a second. An HLS segment repeats once
per segment. A stall that comes back every 20s is covered from its second occurrence on, and it
is forgotten 40s after it stopped coming. Stalls that come in groups are covered from the second
group on, the time between the groups included. A stall that comes back every 90s is a one-off
every time.

Example: a level is reached at x, x+1s, x+10s, then at x+48s, x+49s, x+50s.

| reached at | longest time between occasions | regular until |
|---|---|---|
| x | | not regular: the first occasion |
| x+1s | 1s | x+11s (`keep_min`) |
| x+10s | 9s | x+28s |
| x+48s | 38s | x+124s |
| x+49s | 38s | x+125s |
| x+50s | 38s | x+126s |

From x+28s to x+48s the level is not regular: a pause that long was not seen before. The buffer
may come down in that time (DC7), and the occasion at x+48s can be late. From then on a pause of
up to 60s between two groups is covered (`repeat_max`).

### 4.3 Tracks

- **TR1. Which tracks count.** A track counts from the moment its edge settled for the first
  time, so once its first burst has ended, or `start_timeout` after its first chunk when that
  never happens (a source that stays faster than real time). It counts until 10s after its
  last chunk (`idle`). A removed track stops counting at once. While no track has delivered
  for 10s, the last values stay.
- **TR2. One timeline.** `E` and `L` of the input are combined from the tracks that count
  (2.1). There is one `T` for the input, and it is a function of the pts (DC5), so equal pts
  on two tracks are always presented at the same time.
- **TR3. A track is added.** Its chunks are scheduled on `T` from the first one, and what is
  already late is handled by RL4. It starts to count as TR1 says, so its initial burst does not
  move anything.
- **TR4. A track pauses.** Nothing waits for it. For 10s its edge and lateness still count as
  they were; after that the input is a single-track input. When the track returns it counts
  again at once, with the edge and the lateness it had. If its delivery changed in the
  meantime, the usual rules move them (ED2 at once, ED3 after `W`); a slow first chunk is one
  late chunk.
- **TR5. Reference track** (enforce only). Audio while it counts (TR1), otherwise video
  `[today: audio whenever the input has it]`. A muted microphone hands the reference to video
  after `idle`, and takes it back with its first chunk. The other track is best effort: it is
  part of the target and of the band like any track, but only up to the cut-off of 2.4, and
  it never causes a jump (DC3).
- **TR6. Unknown sync point** (only without the hint "the tracks share one timeline", 7). When
  the edges of the two tracks are 5s or more apart, each track gets its own timeline: its own
  target, band and re-anchoring, by the same rules. Under 3s they are joined again: the
  timeline of the non-reference track (video, when there is audio) is re-anchored to the
  shared one by DC3.

### 4.4 Stage 2: decide (per input)

- **DC1. Start.** `T` is set for the first time when the edge of every track that has
  delivered something is settled (ED4), or `start_timeout` after the first chunk, whichever
  comes first: `T = E + start`.
  - The regular lateness is not known yet, so the first target is `start`.
  - What the initial burst holds beyond `start` is late at that moment (RL4).
  - After the timeout it starts on whatever the edge is. If the burst still goes on, the edge
    keeps falling and DC3 takes over.
- **DC2. Inside the band: nothing**, except DC7.
- **DC3. Outside the band: re-anchor to the target.**

  | the buffer is | and | re-anchor |
  |---|---|---|
  | outside the band | nothing is playing (DC6) | jump |
  | too small | regular packets already miss their time (`B < L`) | jump |
  | too small | nothing is lost yet | slew |
  | too large | over the upper edge by at most `max_slew` | slew |
  | too large | over the upper edge by more | jump |

  - A re-anchor that has started runs until the target is reached, also after the buffer is
    back inside the band. It is re-evaluated on every tick: a slew to later becomes a jump
    when playback gets interrupted, and it follows the target while the target stays on the
    same side of the buffer. When the target ends up on the other side, the re-anchor is
    over.
  - enforce: "regular packets already miss their time" counts for the reference track only.
- **DC4. Slew rate.** `clamp(error / 5s, 1%, 4%)` of content time.
- **DC5. Where a change applies.** `T` is a function of the pts, piecewise linear. A change
  applies to content after the newest pts that was already released, on any track. A slew is
  a ramp of `T` over pts, a jump is a step.
  - A jump to later: the output has nothing new for that long.
  - A jump to earlier: the oldest unreleased content of that length misses its time (RL4).
  - Both tracks read the same function, so no track has to wait for the other one
    `[today: tracks sharing the timeline release in pts order]`.
- **DC6. Nothing is playing** (playback is interrupted): for every track, the presentation
  time of the newest chunk it released has passed. Before DC1 it is always the case. This is a
  fact, not a detection.
- **DC7. What live sync added, it takes back.** When the buffer is above the target because
  of latency that a re-anchor to later added, and the reason for it is gone, it is slewed
  back (DC4). The reason is gone when:
  - the target came back down: the lateness that raised it is no longer regular (LT4); or
  - the rise of the edge that the re-anchor followed was taken back (LT7).
  - Never further than the target, and never by more than was added. It starts only when
    that is more than `tol`. Latency that the stream brought itself stays while it is inside
    the band (DC2): where the start landed (H4), drift, a pts jump that was not stitched.
  - A re-anchor that follows a rise of the edge adds nothing: the stream moved, and the
    buffer is where it was. It counts as added only once the rise is taken back.
  - Only by a slew, and only while something is playing. A stall says nothing about the
    lateness that was learned before it (10.1).

### 4.5 Stage 3: release (per track)

- **RL1. Order.** Chunks leave in the order of the buffer: arrival order on RTMP and HLS,
  (group, index) on MoQ (5.3).
- **RL2. On time.** A chunk is released when it is due within `ahead`. Until then it stays in
  the buffer, where a re-anchor still reaches it (2.1 of `REDESIGN.md`: a released chunk can't
  be taken back).
- **RL3. Hole** (ordered buffers only). When the next chunk in order is missing, release waits
  until the first chunk that is there is due within `ahead`, then skips the hole (M6). On a
  video track the chunk after a hole is a keyframe (5.3).
- **RL4. Late.** The presentation time of the chunk has passed.
  - **Kept**, while nothing is playing (DC6) and a re-anchor to where delivery is now would
    still play it: `pts + c + B* >= now`, with `c` of ED3 over the last `W` (for the input:
    the latest `c` among the tracks). So a late chunk is kept for `B*` after it arrived. This
    is the content that continues where a stall stopped (G7(a)): when ED3 confirms the new
    level, playback goes on from the oldest chunk that is still kept. That is the first one
    after the stall when `B*` is longer than the time ED3 took, a later one with a small
    buffer.
    - Before DC1 the same test with `start`: of the initial burst, what could still be played
      if playback started now is kept.
  - **Otherwise audio is dropped.**
  - **Otherwise video is cut back to its newest keyframe.** Late GOPs that are followed by a
    newer keyframe are dropped. Of the newest GOP the late chunks are released as decode-only
    `[today: decode_only]`, so the decoder is ready for the first chunk that is on time. When
    nothing on time follows, the newest late frame is released to be shown.
- **RL5. Capacity.** A track holds at most 120s of content; beyond that the oldest is dropped.
- **RL6. End of stream.** Everything left is released with its presentation time; the tail is
  not cut.
- **RL7. Deadline for the input.** Live sync tells the input the oldest pts it would still
  take: `now - T` `[today: the same]`, and while nothing is playing `now - (c + B*)`, the
  kept content of RL4, when that is older. For the best-effort video track (TR5) lateness is
  no reason to stop reading: only a newer keyframe is.

## 5. Interface

### 5.1 Public option

The shape stays, one field is added:

```
buffer: number | {
  desired_ms?: number,                  // the starting buffer, and where the buffer returns to
  min_ms?: number,                      // lower limit, for the regular slowest packets
  max_ms?: number,                      // upper limit, for the fastest packets
  max_policy?: "extend" | "enforce",    // MoQ input only
}
```

- `max_policy` (the name is open) says what happens when the stream can't be on time inside
  `max_ms`:
  - `extend`: the buffer grows past `max_ms` and nothing is lost. The only behavior on RTMP
    and HLS, where the field does not exist.
  - `enforce`: playback is never more than `max_ms` behind live, measured on audio when the
    input has audio. Content that arrives too late is not played on time: late audio is
    dropped, late video is shown late.
- The two settings section 1 of `REDESIGN.md` requires:
  - avoid re-anchoring: a wide range (`min_ms` far under `desired_ms`, `max_ms` far over it);
  - force low latency: small values with `enforce` on MoQ. On RTMP small values give "as low
    as delivery allows", and the buffer comes back down when the lateness is gone (DC7).
- Defaults `[today: the same]`: RTMP 1s / 2s / 5s, HLS 2s / 10s / 20s, MoQ 1s / 2s / 5s. How
  missing values are derived does not change. MoQ defaults to `enforce`, which is closest to
  `[today: audio prioritized, video can't push the buffer past max]`.

### 5.2 Input-facing interface

The methods stay as `[today]`: `LiveSync::new(options, sync_point, stats)`, `add_track(kind,
sink)`, `remove_track(kind)`, `flush()`, `LiveSyncTrack::write_chunk(chunk)`,
`LiveSyncTrack::deadline()`. What changes is the options:

```rust
pub(crate) struct LiveSyncOptions {
    pub lower: Duration,
    pub start: Duration,
    pub upper: Duration,
    pub at_upper: AtUpperLimit,     // Extend | Enforce
    pub segmented: bool,            // hint: content is delivered in segments
    pub shared_timeline: bool,      // hint: the tracks share one timeline
    pub tuning: LiveSyncTuning,     // the per-protocol constants of 5.5
}
```

- `write_chunk` never blocks, and the arrival time is taken inside it. The input calls it as
  soon as it has the chunk, so back-pressure further down can't distort what is measured.
- The sink of a track receives `Chunk` (with the pts on the queue clock and the decode-only
  flag) and `Discontinuity` (ST2) `[today: the same two events]`.

### 5.3 Buffer contract

The buffer of a track is the one part with per-protocol implementations
`[today: LiveSyncBuffer; FIFO for RTMP and HLS, ordered by (group, index) for MoQ]`. What live
sync needs from it:

- **Order.** It defines the order chunks leave in (RL1). Live sync never reorders.
- **Holes.** It can say whether the next chunk in order is there, and what the first chunk
  that is there is (RL3). A FIFO has no holes.
  - On a video track a hole ends at a keyframe. On MoQ that holds by itself: frames of one
    group arrive in order, so what is missing is the rest of a group or whole groups
    `[today: the same]`, and a group starts with a keyframe (moq-lite and MoQT require it).
- **Cut.** It can drop from the front up to a pts, and it knows where the keyframes are
  (RL4). This needs a keyframe flag on the video chunk: a new field of the chunk type, set by
  the inputs `[today: no such field; the HLS input tracks GOPs on its own]`. On MoQ it is the
  first frame of a group.
- **Size.** It reports how much content it holds (RL5, stats).

It never looks at the clock and never decides that something is late: that is stage 3.

### 5.4 Stats and events

No internal terms in the names (the edge is "live", the playback timeline is an offset):

- gauges per input: `buffer_ms` (`B`), `target_buffer_ms` (`B*`), `regular_lateness_ms`
  (`L`), `buffer_min_ms` and `buffer_max_ms` (the band), the state (waiting, playing,
  interrupted, re-anchoring);
- counters: re-anchors by kind (start, jump, slew) and by reason (buffer too small, buffer too
  large, content late, interrupted, added latency taken back), late content dropped per
  track (ms), late video shown, discontinuities stitched and undone, time with nothing
  playing;
- every re-anchor is logged once, with its reason and the numbers it was decided on.

### 5.5 Constants

Per protocol:

| | RTMP | HLS | MoQ |
|---|---|---|---|
| `settle` | 500ms | 1s | 500ms |
| `tol` | min(100ms, start / 10) | min(250ms, start / 10) | min(100ms, start / 10) |
| `W_base` | 1s | 2s | 1s |
| `start_timeout` | max(2 x start, 3s) | 30s | max(2 x start, 3s) |
| `max_slew` | 1.2s | 2.4s | 1.2s |
| stitch (4.1) | on | on | off |
| hint: segments | no | yes | no |
| hint: shared timeline | yes | yes | catalog has `clock` |
| `max_policy` | extend | extend | option, default enforce |

- `settle` is `[today: the stabilization period]`. `tol` is `[today: the stabilization
  tolerance]`, made smaller for small buffers.
- `max_slew` is what a slew removes in 30s (HLS: 60s) at 4%.

Shared: memory of the lateness 120s at 250ms resolution, `keep_min` 10s, `repeat_max` 60s
(half of the memory), gap for first sight 500ms, `idle` 10s, capacity 120s, `ahead` 500ms, slew
rate 1-4%, `W` at most 60s and at least four chunks, stitch thresholds 10s forward and 1s
backward, pair window 2s, unknown sync point 5s / 3s.

- `ahead` trades the reach of a re-anchor against the time the decoder gets
  `[today: released up to 5s ahead]`. With a buffer under `ahead` everything is released on
  arrival.

## 6. Detections that can misread

What section 3 of `REDESIGN.md` asks for: every detection that can be wrong, the state a wrong
reading leads into, and how it is left. Seven of them; "nothing is playing" (DC6) and the limits
are facts, not detections.

- **The end of the initial burst (ED4, DC1).**
  - Unlocks: a start that lands on the starting buffer whatever the size of the burst (G1).
  - Misread: the burst pauses for longer than `settle` and then goes on (three HLS segments
    downloaded with a slow request in between). Playback starts on an edge that is too late.
  - State: the buffer is larger than `start` by the part of the burst that came after.
    Inside the band it stays (extra latency inside the limits); over it, DC3.
  - Other direction: delivery so jittery that the edge never settles. Playback starts after
    `start_timeout`.
  - Cost: `settle` is added to the time to the first frame.
- **A new timeline or a burst of old content (ED3).**
  - Unlocks: a backlog after a stall leaves no latency behind (G4, G7(c), G13(c)), and content
    that continues after a stall is followed (G7(a)).
  - Misread 1: a backlog that comes no faster than real time for longer than `W` (a sender
    that can't send faster, a connection that is still recovering). It is taken as a new
    timeline; for the viewer it is one. When it speeds up, the edge falls at once (ED2) and
    the rise is taken back (LT7); DC7 or DC3 removes what the re-anchor added.
  - Misread 2: a real shift is confirmed `W` late. Content of that time is kept as far as
    the buffer reaches (RL4); the freeze is longer by `W`.
  - Misread 3: the first occurrence of a step in delay that comes back is taken as a new
    timeline: content is late for `W`, then one re-anchor. From the second occurrence it is
    lateness (LT7).
  - Misread 4: a step that is known to come back stays for good. It is believed after twice
    its usual length, not after `W_base`. The buffer covers it by then, so nothing is late.
  - Misread 5 (HLS): after a stall inside the download of a segment, the edge can move up
    from the part that arrived, until the next segment on time takes it back (LT7).
  - Cost: `W` (1s; on HLS two segments) before slower delivery is believed.
- **Regular or one-off (LT3, LT4).**
  - Unlocks: one-off problems add no latency (G4, M5), periodic ones are covered (M3, H2).
  - Misread 1: two unrelated stalls of similar size inside 60s count as regular. The buffer
    grows by the size of the stall, by a slew, and stays that way for up to twice their
    distance; then DC7 takes it back.
  - Misread 2: a problem with a period over 60s (`repeat_max`) is one-off every time, and
    every occurrence is late content `[today: covered from the first occurrence, and the
    latency stays]`.
  - Misread 3: the first occurrence of something periodic is always treated as one-off. Cost:
    one interruption more than a design that grows the buffer on the first stall (10.1).
    - The same for the first long pause in it: when occasions come in groups, the level is
      forgotten `keep_min` after the first group, and the second group starts late.
  - Cost: after the conditions get better, the lateness is remembered for twice the longest
    time between its occasions (LT4): `keep_min` when they were close together, like
    jitter, and at most 80s (two occasions 40s apart).
- **First sight (LT5), with the hint.**
  - Unlocks: with segments longer than the starting buffer, playback is interrupted once, not
    twice (H2, H6), and content after a paused source plays at once (H7).
  - Misread: several segments at once after an origin stall are taken as the new normal. The
    backlog is played late, and at the next regular segment the buffer is over the band and
    DC3 cuts it (a skip). In the simulation: 20s of old content played, then a 40s skip.
  - Misread 2: a stall inside the download of one segment is taken for a late segment. The
    buffer grows by a slew until the next segment shows the usual lateness; what the slew
    added is taken back (DC7) when it is more than `tol`.
- **A rise of the edge that is taken back (LT7).**
  - Unlocks: a step in delay that comes back is covered like any other lateness, and a
    misread of ED3 leaves nothing behind.
  - Misread: delivery got slower and then faster for unrelated reasons (two changes of the
    network path). The time in between counts as one occasion of lateness. It is regular only
    if that happens twice, and then the buffer is larger by the step for as long as LT4 keeps
    it.
- **pts discontinuity (ST1-ST4).**
  - Unlocks: a timeline change without an artifact (G6, H8, R5).
  - Misread 1: a jump under the thresholds is not stitched. Forward: a hole of that length and
    that much more buffer (inside the band it stays). Backward: as G7(a).
  - Misread 2: one track skips more than 10s of content without time passing, and it is not
    a new timeline. It is stitched, shown out of sync for up to 2s, then undone (ST4).
- **Unrelated timelines (TR6), without the hint.**
  - Unlocks: tracks that start from unrelated timestamps play at all (G14).
  - Misread: a track that is really 5s behind (M2) gets its own timeline. It plays smoothly
    and out of sync, and it is joined again when it is under 3s behind.

## 7. Hints

- **Content is delivered in segments** (HLS). Advisory. It changes one rule: LT5, a burst after
  a gap is regular at first sight. Without it everything still works, one interruption later
  (section 12: 11.5s instead of 6.0s with nothing playing, for 20s segments on a 10s starting
  buffer).
  - Cost elsewhere: none on other protocols. On HLS: the first-sight misreads of section 6.
- **The tracks share one timeline** (RTMP, HLS; MoQ when the catalog has `clock`). A contract
  when set: TR6 is off, and the tracks are never taken apart, whatever their edges say (H11).
  - It is information from the stream on MoQ (the catalog), resolved in the MoQ input.
- **Keyframe flag** on video chunks. Not a hint about delivery, but new information live sync
  needs (RL4, 5.3).
- Not used: the segment duration, the age of the newest segment (H4), "this gap might still be
  filled". The first two would make the HLS start exact, and FFmpeg does not expose them
  (section 11).

## 8. Scenarios, one by one

All protocols:

- **G1 Start.** DC1. The time to the first frame is the burst plus `settle`, or `start` when
  there is no burst. The buffer lands on `start` whatever the size of the burst, and the part
  of the burst older than that is not played (RL4). A burst that does not end: `start_timeout`.
- **G2 Normal flow.** DC2.
- **G3 A/V sync.** TR2, DC5: one function of the pts for both tracks.
- **G4 Slew or jump.** DC3. A one-off burst never makes the buffer grow (LT3), so there is
  nothing to take back. A periodic problem is covered from its second occurrence (LT3, LT4),
  and what it added is taken back when it has ended (DC7). A new timeline against a burst:
  ED3. A step in delay that comes back: LT7.
- **G5 Clock drift.** The edge follows it (ED2 at once, ED3 in steps of `tol`). Nothing moves
  inside the band; at an edge of the band the buffer is slewed back to the target.
- **G6 pts discontinuity.** Stitched (4.1): no artifact, nothing forgotten. Not stitched (under
  the thresholds, or MoQ): forward, the edge falls at once and the buffer is larger by the
  jump (inside the band: a hole and that much latency; over it: DC3). Backward: as G7(a).
- **G7 Stall.**
  - (a) Content continues: it is late, so it is kept (RL4). ED3 confirms the new level after
    `W`, DC3 jumps, and playback goes on from the first chunk after the stall, or with a
    small buffer from the oldest one that is still kept.
  - (b) Content skipped: it is on time, nothing happens.
  - (c) Backlog: its late part is dropped (RL4), the rest plays on the old timeline.
- **G13 Gap covered by the buffer.** (a) The edge moves up after `W`; the buffer is smaller
  by the gap; DC2, or a slew back when it is under the band. (b), (c) Nothing.
- **G8 Not real time.** Faster: the edge keeps falling, and each time the buffer is over the
  band by `max_slew` it is cut with a jump. Slower: the buffer is slewed up at 4%, and when
  that is not enough, playback runs out and jumps back to the target (DC3). A file pushed as
  fast as possible is the same, with RL5 as the limit on memory.
- **G9 Track added or removed.** TR3, TR1.
- **G10 Single track.** The same rules with one track; TR4 for a track that stopped for good.
- **G11 B-frames.** The reorder depth is lateness like any other (LT3). It is far under the
  stitch thresholds.
- **G12 End of stream.** RL6.
- **G14 Unknown sync point.** TR6. The broken DTX stream is handled only this way: once its
  audio is 5s off, the tracks are taken apart.
- **G15 Delivery does not fit.** extend: the band moves up (2.4). enforce: the cut-off, and
  what does not fit is late.
- **G16 A track pauses.** TR4. Nothing is re-anchored: for 10s the values stay, and after that
  the timeline moves only if the buffer of the remaining track is outside the band. The track
  that returns has the edge it had. In enforce the other track is the reference while audio
  is paused (TR5).

HLS:

- **H1.** DC1 does not wait for a segment: the first frame comes `settle` after the batch.
- **H2.** Playback runs out when the next segment comes later than `start` after the batch.
  Its first chunk is regular at first sight (LT5), nothing is playing, so DC3 jumps and the
  segment plays from its start, `lower` after it arrived. One interruption.
- **H3.** RL4.
- **H4.** The buffer lands between `start` and `start` plus one segment, depending on when the
  next segment is published. That is inside the band, so nothing is re-anchored.
- **H5.** Late segments are covered by `lower`, and by the lateness that repeats (LT3). One
  that is later than that: as H2.
- **H6.** Longer: as H2, one interruption. Shorter: the lateness shrinks after LT4. The buffer
  is re-anchored if it is over the band then (DC3); otherwise it comes down by what was added
  for the longer segments (DC7).
- **H7.** Source paused: as H2, playback resumes `lower` after the first segment; the edge
  follows later, without a re-anchor. Several segments at once: the first-sight misread of
  section 6. Segments skipped by FFmpeg: it skips because time passed, so the content moved
  no further than the clock and nothing is stitched. It is G7(b), and the segments still
  listed follow as a burst (G7(c)).
- **H8.** G6.
- **H9.** G8, slower.
- **H10.** Works: the target becomes one segment plus `lower`.
- **H11.** The edges of the tracks differ, the input uses the later one (TR2), and the hint
  keeps the tracks together.

RTMP:

- **R1.** Always extend: no track is given up.
- **R2.** G1.
- **R3.** G7(c). A longer gap is not a different case here: late content is dropped, and when
  the timeline has moved, playback is re-anchored `W` after delivery resumed (9.1).
- **R4.** No jump for ST1 (time passed while the frames were missing), so nothing is stitched,
  and the edge of video does not move.
- **R5.** G6. **R6.** G8. **R7.** TR3. **R8.** A new instance: G1.

MoQ:

- **M1.** enforce: the timeline of audio is held by the cut-off whatever video does; video is
  shown late. What the lagging video added to the buffer is taken back when it recovers
  (DC7). Everything lost for a while: nothing is playing, and on resume G7.
- **M2.** enforce is the reference track of M2 (TR5). Late audio is dropped. Late video is
  shown as the newest frame of what arrived (RL4): frames that arrive one by one all show, of
  a bunch only the last one. extend is "everything is always delivered".
- **M3.** Nothing to re-anchor when video is deliverable again: its chunks are on time on the
  timeline that audio kept. Video resumes at its next keyframe (RL4).
- **M4.** The lateness grows (LT3), DC3 slews the buffer up. Past `upper`: G15.
- **M5.** Seen once: nothing moves; the late part is dropped.
- **M6.** RL3. **M7.** RL1; stitching is off, so a late group is not a discontinuity.
- **M8.** The edges of both tracks move up (ED3), DC3.
- **M9.** No controls: audio is the reference in enforce; in extend playback waits for both.
- **M10.** R8. **M11.** TR3, TR1. **M12.** G1.

## 9. What is given up, what differs, what was not listed

### 9.1 Requirements given up or changed

- **G2 gets one exception: latency that live sync added itself is taken back, also inside
  the limits** (DC7). G2 as written would leave it there: after a periodic problem ended
  (G4), after a track lagged for a while (M1), after a detection misread.
  - Why: this latency comes neither from the stream nor from the limits of the user. It comes
    from the detections of this design, and every one of them would leave its trace until the
    buffer happens to cross the upper limit. M1 also asks for it ("what is left is slewed").
  - What stays as G2 says: latency that the stream brought itself (H4, drift, a pts jump that
    was not stitched).
  - Cost: one more number to keep (how much was added), and a slew inside the limits.
- **A longer gap is not synchronized "as at the start"** (R3). After any gap the old timeline
  is kept while it fits, and the buffer is not put back on `start`.
  - Why: it would be the same re-anchor in most cases, and where it differs it drops content
    to remove latency that is inside the limits.
- **A backlog of several HLS segments is played late and then skipped**, not dropped at once
  (G7(c), H7). The acceptable variant of G7(c).
  - Why: at the first chunk it can't be told from a source that was paused (G7(a)), and
    there waiting costs two segments of freeze.
- **The HLS start does not land on the starting buffer precisely** (H4), as expected there.
- **The broken DTX stream (G14) is handled only through TR6**, so not when the catalog claims
  a shared clock.
- **The leading track can't be selected** (M9). The reference is audio.
- **A periodic problem with a period over 60s is not covered** (M3; `repeat_max`, LT3). Every
  occurrence is a one-off.
- **The extreme low-latency mode** (play as received, section 5 of `REDESIGN.md`) is not part
  of this. enforce with small values is the closest.
- **A source faster than a slew absorbs** (G8) is shown with a skip every time the buffer is
  `max_slew` over the band, without a smarter resync.

### 9.2 Differences between protocols

- **`max_policy` exists only on MoQ.** Only there the transport can deliver one track without
  the other and drop parts. On RTMP and HLS a cut-off makes nothing arrive sooner, and giving
  up a late track is not acceptable (R1).
- **Stitching is off on MoQ.** Groups arrive out of order, so pts that go back are normal
  there, and a restart of the publisher is a new broadcast anyway. A jump inside a broadcast
  is still handled, with an artifact (G6).
- **The hints** (7) and **the constants** (5.5): HLS delivers in segments and is not expected
  to be low latency, so it waits longer (`settle`, `W_base`, `start_timeout`) and slews
  further before it jumps (`max_slew`).
- **The defaults** of `buffer` `[today: the same]`.

### 9.3 Requirements that were not listed

- **Replayable.** Decisions depend only on the chunks and on the clock passed in (section 3).
- **Arrival is measured where it happens.** `write_chunk` does not block and takes the time
  itself. A hiccup of our own process looks like a stall with a backlog (G7(c)) and leaves
  nothing behind.
- **Every state has a way out** that does not need a restart of the input (section 6).
- **No oscillation.** The band with a target inside it, `tol` on every comparison, a
  re-anchor that runs to the target, and LT4 against shrinking in every good period. DC7 only
  goes down, and only as far as the target.
- **Bounded memory** (RL5), also when a source never stops bursting.
- **A broken timestamp** moves nothing by itself (ED2), and on RTMP and HLS it is stitched
  away and back.
- **Explainable.** Every re-anchor has one reason out of a short list (5.4).
- **A/V sync through a slew.** Both tracks read one function of the pts (DC5).
- **Sparse tracks** (a frame every few seconds): between frames nothing is playing, so a
  re-anchor there is free and invisible. The one special case is the window of ED3, which is
  counted in chunks when there are too few of them.

## 10. Choices inside the design, and other designs

### 10.1 Choices inside the design

- **Late content after a stall is dropped; the timeline is not moved to it** (rule 5). The
  alternative, used by most players: resume where the content is and remove the latency
  afterwards. It needs a rule for getting back, and it leaves latency behind exactly where
  G4 does not want it. It costs `W` in G7(a), and one more interruption for a problem that
  turns out to be periodic.
- **"Seen twice" instead of a percentile.** A percentile of recent delays (the usual choice,
  section 11) lets rare but regular lateness be outvoted: the first chunk of a 20s segment is
  one sample in hundreds. And one large sample stays in it for as long as the memory is.
- **Lateness is kept for twice the longest time between its occasions, not the latest one**
  (LT4). With the latest one, a group of close occasions sets the keep time to `keep_min`
  whatever the pause before the group was, and with DC7 the buffer then goes down after every
  group and up again at the next one. The cost: lateness whose occasions were far apart is
  forgotten later. Jitter is not affected.
- **Regular means: it comes back within 60s, and that is read from the history every time**
  (LT3, LT4). The alternative is a keep time that is stored at the occasion and outlives the
  history: it covers periods up to 120s. In the simulation it also held the buffer for minutes
  after a pattern had ended, and the state is one more thing that can be stale. Covering
  slower patterns without it needs a memory twice as long, and then two unrelated one-offs up
  to 120s apart hold latency for up to 160s. The cost of the choice: a stall that comes back
  every 90s interrupts playback every time.
- **Delays within a tenth of each other are the same level** (LT2), and the buffer is sized
  for what two occasions both reached (LT6). Without the first, the occasions of the highest
  levels are few and far apart, and the keep time follows that distance instead of the real
  period. Without the second, one occasion that went a little higher sizes the buffer (1.75s
  more after an HLS origin stall in the simulation).
- **Growing never jumps while nothing is lost; shrinking jumps only past `max_slew` over the
  band.** A jump to grow is a freeze for nothing. A source that runs 3% fast reaches the
  upper edge again and again, and has to be slewed back each time (G5).
- **No re-centering while nothing is playing.** It would be free, but it shrinks the buffer
  right after an interruption, which made the next one longer in the simulation. DC7 follows
  the same choice: during a stall the lateness that was learned runs out of its keep time
  (LT4) without any delivery that could show it is gone.
- **The window of ED3 follows how long regular lateness lasts**, read from the same history,
  instead of a separate measurement of arrival gaps `[today: 4 x the longest gap of the last
  60s]`: one estimator less. Its height (`2 x L`, the first version) is the wrong measure: a
  step in delay is low and long.
- **Lateness is measured against the edge of now, not the edge of its own time** (LT7). The
  second is exact under drift and wrong for a burst: while a segment is downloaded the edge
  can still fall, and the first chunk of the segment is late against where the download
  ends. In the simulation it broke the HLS start. The cost of the first: under drift, old
  lateness reads higher by the drift during the time LT4 keeps it (0.3s at 3%).
- **The edge follows slower delivery after a fixed window, also when a backlog might still
  come.** A backlog that comes faster than real time is waited for; one that comes no faster
  is followed, because waiting for it would mean showing nothing for as long as it takes.

### 10.2 Other designs

**B. Driven by interruptions** (what HLS and DASH players do).

- State: the edge, and a target that starts at `start`.
- Rules: start as DC1. When playback runs out, it resumes where the content is, and the target
  grows by a step or by the measured shortfall. The target comes down slowly after a time
  without interruptions (a minute, or half of the excess each time). Over the upper limit:
  slew or jump as DC3. There is no lower limit: running out is the only sign that the buffer
  is too small.
- For: very little state, and no detection that can misread.
- Against: every new delivery pattern costs a visible interruption, also plain jitter on MoQ.
  A one-off stall leaves latency until the decay removes it (G4, G7(c), R3). The decay time is
  the remember-or-forget problem again: too short and a periodic problem keeps interrupting,
  too long and latency stays for minutes.
- When to prefer it: only HLS matters, and interruptions at the start are acceptable.

**C. Fixed delay** (SRT, GStreamer `rtpjitterbuffer`, the first MoQ players).

- State: the edge. `T = E + start`, always. Late content is dropped. The edge follows drift.
- For: nothing to learn and nothing to misread. Fully predictable.
- Against: the user has to size the buffer for the stream. H2 fails by definition (20s
  segments on a 10s buffer run out on every segment), and so does G15.
- It is this design with `lower = 0` and enforce, so it exists as a configuration.

**D. Continuous control** (WebRTC NetEq).

- State: the edge over a short window, a histogram of lateness with exponential forgetting.
  The target is a high percentile of it, and playback is stretched or squashed all the time
  to sit on the target.
- For: the best latency for a given loss rate; made for conversations.
- Against: there is no band, so content is stretched or squashed nearly all the time (G2).
  Rare regular lateness is outvoted (NetEq had a separate peak detector for it). A one-off
  spike stays in the histogram for tens of seconds (G4).

Where this design sits: the reference of C, the split into two estimators of D with its old
peak detector in place of the percentile, and the band and the free re-anchor of the players.

### 10.3 Today's implementation, and what its parameters can do

`[today]`, the same on master and on this branch unless said:

- State: per track, the smallest delivery delay of a window (the edge) and the largest one
  among the chunks that arrived while the edge was stable (the slowest). The window is
  `clamp(4 x the longest arrival gap of the last 60s, 20s, 120s)`.
- The target and the band have the shape of 2.4: `max(edge + start, slowest + lower)`.
- The target is set only while the buffer is outside the band, and it is reached by a slew
  only (1-4%).
- A track is reset (buffer flushed, estimate cleared, started again) when it ran out of content
  for 1s, when the edge of the last 2s is later than the edge by a threshold (RTMP 2s, HLS 10s,
  MoQ 500ms), and at a pts jump of 10s.
- master: one more estimator that every chunk feeds and no stall clears, and the playback
  timeline is kept over a reset.
- This branch: no such estimator, so when every track is reset nothing is left. And a priority
  track (audio on MoQ) whose upper limit wins, which is the core of `enforce`.

Three differences from this design decide the results below:

1. **Lateness is the largest delay of one window.** Everything that was seen once counts. And
   one length has to do two jobs: forget a one-off, remember what repeats.
2. **The target is sticky, and re-anchoring is a slew.** What was added inside the limits
   stays. A large excess takes minutes. An interruption that is already visible is not used.
3. **A stall is handled by forgetting.** On this branch everything is forgotten. On master the
   lateness is kept, and then it includes the backlog.

Both were run through the scenarios of section 12, as a model in the same script (`python3
design_sim.py master|master10|branch ...`; `master10` is master with the 20s of the window
changed to 10s). A plain number is the buffer of the fastest packets; "interrupted" is the time
with nothing playing; "loops" is running out on every segment and starting again on the
starting buffer.

| scenario | master | this branch | this design |
|---|---|---|---|
| RTMP: 5s stall, backlog at 5x | restart, 6.0s; 4.6s 85s later | restart, 2.0s | untouched, 2.0s |
| RTMP: 5s stall, content continues | restart, ends at 1.05s | restart, 2.0s | one re-anchor, 2.0s |
| RTMP: 1.5s gap, then a burst | 2.5s, stays | as master | untouched, 2.0s |
| RTMP: 2s stall every 20s | 3.0s at the first, stays | as master | 3.0s at the second, then 2.0s |
| RTMP: 2s stall every 90s | 3.0s at the first, stays | as master | 2.0s, 0.25s lost each time |
| HLS: 20s segments, 10s buffer | interrupted once, 20s | loops | interrupted once, 20s |
| HLS: origin stall, 3 segments | 60s; 56.8s 170s later | loops | 40s skip, then 20s |
| HLS: one download stalls for 8s | 11.4s; 12.7s 2 min later | as master | 11.4s |
| MoQ: delays up to 400ms for 40s | 0.47s, stays | as master | 0.45s, then 0.25s |
| MoQ: one 1s hiccup | 1.0s 20s later, then 0.25s | as master | untouched, 0.25s |
| MoQ: 0.6s hiccup every 15s | 4 resets, interrupted 1.8s | 4 resets, 3.4s | no reset, 0.8s |
| MoQ: three hiccups every 40s | 3 resets, interrupted 4.5s | 3 resets, 5.7s | no reset, 1.3s |
| source 3% slow | buffer sinks to 0.14s | as master | 1.8s, steady slew |
| pts jump of 30s, HLS discontinuity | restart, nothing lost | as master | stitched |

The start, steady delivery, jitter, short HLS segments and a source 0.1% off are the same in all
three.

**What the length of the window changes.** With 10s instead of 20s, three things move:

- One 1s hiccup on MoQ: 0.47s instead of 1.0s 20s later. The excess over the upper limit is
  forgotten 10s sooner.
- A 0.6s hiccup every 15s: interrupted 2.5s instead of 1.8s (three hiccups every 40s: 5.8s
  instead of 4.5s). The hiccup falls out of the window before the next one.
- A source 3% slow: the buffer sinks to 0.43s instead of 0.14s. The edge of a window is stale
  by the drift of one window.

The rest stays within 0.1s, for three reasons:

- After a stall the window is 4 x the gap, so the 20s do not apply: 20s after a 5s stall, 80s on
  20s HLS segments.
- The target is set only outside the band. What a one-off added inside the limits stays, whatever
  the window forgot.
- A shorter window forgets what repeats. A longer one keeps every one-off longer. No length does
  both, which is difference 1.

**What each benefit takes on master.**

| benefit | rules | on master |
|---|---|---|
| added latency comes back when its reason is gone | DC7 | local: the correction decision |
| a jump when nothing plays, or it is too far to slew | DC3, DC6 | local: the correction decision |
| a one-off adds nothing; a backlog is not slow delivery | ED3, LT2-LT7 | new estimator (stage 1) |
| a stall forgets nothing; long HLS segments | LT5, no reset | stage 1, and the hint |
| a pts jump without a restart | ST1-ST4 | independent of the rest |
| the upper limit wins on one track | TR5, `enforce` | on this branch already |

- The first two are local changes to today's code: count what was added and set the target also
  inside the band; jump instead of slew in two cases. No parameter gives them. With them, a
  one-off still adds latency, for as long as the window holds it.
- Taken alone, the first one has a cost. The target that stays is what covers slow patterns
  today. Without it, a problem that comes back less often than the window is long (20s, or
  4 x the gap) makes the buffer go down and up between occurrences. This is reasoned from the
  code, not simulated.
- The third is the part that needs this design. It is one component: what today's estimator
  reports as the slowest.
- The fourth is what makes the reset unnecessary. Without the third it is what master does,
  and the backlog then counts as slow delivery (the 6.0s of the first row).
- None of these changes was written or tried. "Local" is a reading of the code.

**Where today's implementation is ahead.**

- A problem that repeats is covered from its first occurrence (this design: from the second).
  Inside the limits with any period, because the target stays (this design: up to 60s apart);
  over the upper limit only while the window holds it.
- After a stall, master plays more of the backlog: 2.5s dropped instead of 5.9s on RTMP, 57s
  instead of 91s after the HLS origin stall. The price is the latency that stays.
- A pts jump of 10s or more is handled as well by a restart as by stitching, in these
  scenarios.
- It exists and runs. This design is a document and a model, and its constants are first
  values.

**Limits of the comparison.**

- Today's side is a model written from the Rust code, not the Rust code. The estimator, the
  band, the slew, the resets and the start are modeled; the decoder and the queue are not.
- One track only. Two tracks (shared and independent mode, the priority track) are not
  compared.
- The loop on this branch with HLS segments longer than the buffer is the one G7 of
  `REDESIGN.md` describes. It was seen in the model only.
- The resets in the MoQ rows come from the check on the last 2s: it reads the current 2s
  bucket, so a hiccup over 500ms that crosses a bucket boundary leaves it with late chunks
  only. Also seen in the model only.

## 11. Where the rules come from

Each of these was read in the source or the documentation of the project while preparing this
proposal (local clones of libwebrtc, FFmpeg, hls.js, OBS and moq; the rest online).

- **The edge as a minimum over a short window (ED2, ED3).**
  - NetEq: delay relative to the fastest packet of the last 2s (`packet_arrival_history.cc`).
  - GStreamer `rtpjitterbuffer`: minimum over 2s or 512 packets (`calculate_skew`).
  - MoQ `hang` player, current main: the reference moves later only when every arrival in a
    2s window trails it (`js/watch/src/sync.ts`, `doc/concept/audio-jitter.md`). ED3 is this
    rule plus "not catching up".
- **Two estimators with different memory (rule 1).** NetEq: 2s for the reference, about 20s
  half-life for the target (`delay_manager.h`). WebRTC video: `TimestampExtrapolator` and
  `JitterEstimator`.
- **Seen twice, kept for twice the period (LT3, LT4).** NetEq's former `DelayPeakDetector`:
  peak mode after two peaks at most 10s apart, ended after twice the longest period without
  one (`delay_peak_detector.cc`, branch m79).
- **A band and a small rate (DC2, DC4).** ExoPlayer: speed `1 + 0.1 x error`, clamped to
  0.97..1.03, 20ms dead band (`DefaultLivePlaybackSpeedControl`). Shaka: 0.95 / 1.1 outside a
  tolerance of 0.5s. NetEq stretches about 100ms per second.
- **A jump when it is too far (DC3).** hls.js seeks past `liveMaxLatencyDuration`; dash.js
  past `maxDrift` (12s); GStreamer resyncs at 1s; WebRTC video resets at 10s.
- **Late is dropped, the time base does not move (rule 5).** SRT (too-late packet drop),
  WebRTC video (a late frame is dropped when a newer one can be decoded), `hang` (old groups
  are skipped).
  - The opposite, resume in place and raise the target: hls.js (+1s per stall), ExoPlayer
    (+500ms per rebuffer), VLC and OBS (the delay is only ever increased). That is design B.
- **Stitching (4.1).** The ffmpeg CLI subtracts a dts jump over 10s from all later timestamps
  (`dts_delta_threshold`). SRS makes a jump over 250ms continuous. hls.js and ExoPlayer keep
  one timestamp offset per discontinuity, placed after the previous segment.
  - Thresholds elsewhere: VLC 300ms, OBS 2s, mpv and MediaMTX 5s, ffmpeg and WebRTC 10s.
- **One mapping for both tracks (TR2).** OBS, VLC and `hang` use one; ffplay and mpv make
  audio the master. None places the tracks independently.
- **What FFmpeg gives an HLS input** (`libavformat/hls.c`): it starts 3 segments back, hands
  out a segment as fast as it downloads, and exposes neither the segment duration nor
  `EXT-X-PROGRAM-DATE-TIME` nor discontinuities; a discontinuity arrives as a bare pts jump.
  The players above measure latency against the playlist, which is not possible here.

## 12. What was checked in a simulation

`design_sim.py` (next to this file) is a model of the rules in about 450 lines of Python: chunks
are (arrival, pts) pairs, there is no media, decoder or queue. It was used to find rules that
did not work, not to tune values. `python3 design_sim.py [rtmp] [hls] [moq] [av] [misc]
[review]`. The same file has the scenarios and a model of today's implementation (10.3):
`python3 design_sim.py master|master10|branch [rtmp] ...`.

Not modeled: two-track stitching (ST3, ST4), TR6, keyframes and decode-only (RL4), holes (RL3),
capacity (RL5), the deadline (RL7), and DC5 (a re-anchor is applied to all unreleased content
at once). What DC7 takes back is counted for the input as a whole, not per track.

Results of the run of 2026-11-25, with the rules as section 4 has them (after the review).

RTMP, 1s / 2s / 5s:

- 3s burst at the start: first frame after about 1s, buffer 2.00s from then on.
- 5s stall, backlog at 5x: no re-anchor; 4.2s of the backlog dropped; buffer 2.00s.
- 5s stall, content continues: re-anchored 0.5s after delivery resumed; nothing dropped.
- 5s stall, content skipped; 1.5s gap with a burst after it: nothing.
- 0.6s gap, content continues: buffer 1.40s, no re-anchor. 1.5s gap: buffer 0.50s, slewed
  back to 2.00s in 44s.
- 2s stall with a backlog every 20s: nothing on the first one, a slew to 3.00s from the
  second; nothing is late after that. 40s after the last one it is slewed back to 2.00s (DC7).
- The same every 50s: covered from the second one, and slewed back 70s after the last one
  (the older occasion leaves the history, LT4). Every 90s: never regular; the buffer stays at
  2.00s and about 0.25s is lost at every stall.
- Delay 1.5s higher for 4s, every 12s: a slew during the first step, taken back when the
  step ends; from the second step buffer 2.52s. No jump, nothing dropped. About 25s after
  the last step it is slewed back.
- Video only, one frame every 3s, delay goes up by 3s for good: the edge follows after four
  frames, one re-anchor while nothing is shown, frames on time from then on.
- pts jump of 30s in either direction, or of 3s backward: stitched, buffer unchanged. Without
  stitching, 30s in either direction: one jump, 1.9s with nothing playing.
- pts jump of 3s forward (under the threshold): a 3s hole, and the buffer stays at 5.00s.
  0.5s backward: the edge follows within 1s, buffer 1.50s, no re-anchor.
- Source 0.1% fast: no re-anchor in 240s. 3% slow: a steady 3% slew, buffer about 1.8s. 10%
  slow: playback runs out and jumps back about every 27s.
- Source 3% fast: the buffer reaches the upper limit after 104s and is slewed back; no jump in
  540s, and it ends in a steady 3% slew with the buffer at 2.15s. 10% fast: slewed at the
  upper limit, and a 4.2s skip about every 50s.

HLS, 2s / 10s / 20s:

- 6s segments: first frame at 2.8s (1.8s of download); buffer 11.2s or 14.2s depending on
  when the next segment comes; no re-anchor. The same with 0-1.5s of arrival jitter.
- 20s segments, next segment 20s after the start: one interruption (6.0s with nothing
  playing), then buffer 20s. Next segment 8s after the start: no interruption, buffer 26s.
- The same without the segments hint: two interruptions, 11.5s.
- 2s segments on 2s / 3s / 6s: no re-anchor, buffer 3.9s.
- Segments change from 6s to 20s: one interruption.
- Source paused for 60s: playback resumes 2s after the first segment, nothing dropped.
- Origin stall, three segments at once: the backlog plays 40s late; 20s later a 40s skip.
- One download stalls for 8s half way through a segment: nothing dropped, no interruption;
  4s of slewing, buffer 11.2s before and 11.4s after.
- pts jump at a segment boundary: stitched, no artifact. Without stitching: one jump, 9.8s
  with nothing playing.

MoQ, 100 / 250 / 500ms:

- Delays of up to 400ms start: the buffer goes from 0.25s to 0.45s, nothing lost. When they
  stop it is slewed back to 0.25s, starting 7s after the last one and done within a minute.
- Delay 300ms higher for 3s, every 8s (extend and enforce alike): the first step is taken for
  a new timeline (0.7s of audio late, one re-anchor while nothing plays) and taken back when
  it ends. From the second step the buffer is 0.42s and nothing is late. About 15s after
  the last step it is slewed back to 0.25s.
- One 1s hiccup: no re-anchor; 1.1s lost.
- A 0.6s hiccup every 15s, extend: buffer 0.70s from the second one; slewed back to 0.25s
  30s after the last one. enforce: 0.50s, and about 0.1s lost per hiccup.
- Three 0.6s hiccups 1s apart, every 40s, extend: buffer 0.70s from the second hiccup of the
  first group, and slewed back 10s after that group. From the second group it stays at 0.70s
  through the pauses, and it is slewed back 76s after the last group (LT4).
- Video delivered at 70% of real time for 30s, audio on time. enforce: audio is not touched
  apart from a slew to 0.50s, and back to 0.25s after video recovered; video is shown late
  for 42s and is in sync again a few seconds after it recovers. extend: 37 jumps in 30s,
  audio included; after video recovers the buffer (1.3s) is slewed back to 0.25s.
- Audio received 0.8s before video, enforce: video is always shown late, about 0.3s behind.
- Audio pauses for 30s while video plays (RTMP limits): no re-anchor.
- Video only, one frame every 3s, delay goes up by 0.5s for good: as on RTMP, on time again
  after four frames.
- Source 8% fast, so that the edge never settles: starts at `start_timeout`, then a 1.45s
  skip every 33s (G8).

## 13. Decisions to review, and what is left open

Decided in this proposal, each with the alternative that was not taken. The first two shape the
rest; the others are local.

1. **Late content after a stall is dropped; the timeline does not move to it** (rule 5, 10.1).
   Not taken: resume where the content is and remove the latency afterwards (design B).
2. **Latency that live sync added itself is taken back inside the limits** (DC7), an exception
   to G2 (9.1). Not taken: G2 as written, where it stays until the buffer crosses the upper
   limit.
3. **The public option**: one new field, `max_policy`, on MoQ only, default `enforce` (5.1).
   The leading track is not selectable: in enforce it is audio while audio delivers (M9).
   Not taken: a second option for the leading track (audio, video, none).
4. **A keyframe flag on the chunk type** (5.3), so that cutting late video is done in one
   place. Not taken: the cut stays in each input `[today: HLS only]`.
5. **extend on MoQ with one track slower than real time is left as it is**: it follows the
   slow track with a jump every second while that lasts (37 jumps in the simulation). The
   user chose "everything is delivered". Not taken: let TR6 take the tracks apart also when
   the catalog says they share a clock.
6. **Late video is shown as the newest frame**, so late video that arrives in bunches shows
   one frame per bunch (M2). Not taken: a timeline of its own for late video in enforce,
   smooth and out of sync. It is the same mechanism as the alternative of 5.
7. **Several HLS segments at once are played late and then skipped** (9.1). Not taken: a cap
   on first sight (for example twice the regular lateness), which makes the freeze after a
   paused source longer.
8. **Lateness is regular when it comes back within 60s, and it is kept for twice the longest
   time between its occasions** (LT3, LT4, 10.1). Not taken: twice the latest time, which
   lets the buffer go down and up between groups of occasions. Also not taken: covering
   periods up to 120s, with a stored keep time or a longer memory; a stall every 90s is a
   one-off every time.

Left open, because it needs a measurement and not a decision:

- **`ahead`** (500ms): how much time the decoder really needs.
- **The constants of 5.5.** They are first values. The simulation was used to find rules that
  do not work, not to tune them.
- **What the simulation does not cover** (12): two-track stitching, unrelated timelines,
  keyframes, holes, the deadline for the input.

## 14. Review findings

A review of this proposal against sections 1 and 2 of `REDESIGN.md`, by a different session
(2026-11-25), done by reasoning through the rules, not by running the simulation. Ordered by
severity. The scenario tag after each title says where it shows up.

1. **Delay steps that come back are seen neither as lateness nor as a shift** (bad network:
   cellular base delay switching, route flaps, Wi-Fi power save; steps of 100-500ms every few
   seconds).
   - A step up that lasts longer than `W` becomes the edge (ED3), so LT3 never sees it as
     lateness; the step down moves the edge back at once (ED2). Every step up then costs late
     content and a re-anchor after `W` (MoQ: about 1s of freeze per step), and the pattern is
     never learned, although it is the periodic problem of M3.
   - LT7 shifts the history only when the edge goes up. After each step down, samples that were
     shifted count as lateness of up to twice the step for as long as LT4 keeps them, so `L`
     depends on where in the cycle it is read. The same asymmetry inflates `L` after an
     unstitched forward pts jump (MoQ) and under fast clock drift (edge falls, history stays).
   - Direction: an edge that moved up and came back is a recurring step, counted as lateness of
     the higher level (LT3 over edge moves, not only over chunks); history kept relative to the
     edge, so that no shift is needed in either direction.
2. **Sparse tracks** (static screen share sending frames only on change, audio with DTX; 9.3
   claims they need no special case).
   - ED3 needs delivery without an arrival gap longer than `0.75 x W` (0.75s while `L` is
     small), and LT2 needs faster delivery between two occasions. A track with a frame every
     few seconds meets neither: its edge never moves up and its lateness is never regular.
   - Video-only input of that kind: after the delivery delay steps up, every frame is late for
     good (shown late, as the newest frame), and nothing re-anchors, because `B` is computed
     from the stale edge. With a second track that delivers, the input edge hides it.
   - Direction: coverage counted in chunks rather than in time (the last N chunks or `W`,
     whichever is longer).
3. **The reference track while it pauses** (TR5, G16, MoQ). The reference is "audio when the
   input has an audio track". With a muted microphone (no audio packets) the cut-off is measured
   on a stale audio edge; when video gets later in the meantime it is best effort against a
   track that delivers nothing, and is shown late while nothing else plays. Direction: audio
   while it counts (TR1), otherwise video `[today: video while there is no audio]`.
4. **A track that returns** (TR4 against TR1, G16). TR4 counts it again from its first chunk,
   TR1 only once its edge settled. From the first chunk: ED1 takes the edge from it, and after
   a pause the first video chunk is a keyframe, the largest and slowest frame. `E` of the input
   steps up by its extra delay (MoQ: easily a few hundred ms), `B` drops, and with a small
   buffer `B < L` gives a jump. G16 expects no re-anchoring when the track resumes with the same
   delay. Direction: a returning track counts after its edge settles (as TR1), or keeps its old
   edge.
5. **A track whose edge never settles** (G8, very jittery delivery). ED4 is never true for a
   source more than `tol / settle` fast (5% with a 250ms starting buffer). DC1 still starts at
   `start_timeout`, but TR1 never lets the track count and LT8 never records its lateness, so
   `E` and `L` of the input are undefined when no track counts. Fallback needed: a track counts
   from `start_timeout` at the latest.
6. **First sight on a bad HLS network** (LT5, G4). Any arrival gap of at least 500ms counts,
   also a TCP stall inside a segment download. Its chunk is regular at once, so a one-off stall
   grows the buffer, and once the next segment replaces the value, the added buffer stays as
   long as it is inside the band (item 7). Direction: first sight only for a gap comparable to
   the previous first-sight gap (a segment), or a first sight that did not repeat expires.
7. **Latency left inside the band reaches further than 9.1 says** (G2 against M1). 9.1 cites G4
   and M5, but the same rule decides M1: with the MoQ defaults (1s / 2s / 5s, enforce) a period
   of lagging video moves the buffer up to 5s, and after recovery it stays there, while M1
   expects a good latency within seconds. The misreads of items 1 and 6 and of LT3 (two
   unrelated stalls) end the same way. Since this latency comes from the design's own
   detections and not from the user's limits, the alternative of 13 fits the exception G4
   already makes to G2.
8. **Kept content and the deadline** (RL4 against RL7, G7(a) on MoQ). While nothing plays, RL4
   keeps late content that a re-anchor would still play, but the input is still given
   `now - T`, and the MoQ reader drops a group once a newer group has started behind that
   deadline `[today: track_reader.rs, drop_late_groups]`. Before ED3 confirms the new level most
   of the kept content is gone, and playback resumes at the newest group, not "from its first
   chunk" (8, G7). The outcome is acceptable; either the deadline follows the kept test while
   nothing plays, or section 8 says what happens.
9. **Keyframe after a hole** (RL3, 2.1 of `REDESIGN.md`). RL4 cuts video at keyframes, but RL3
   skips a hole inside a GOP without that: the decoder gets frames without their references
   until the next keyframe. It has to say where the keyframe is ensured (in live sync or in an
   element after it; 2.1 allows both).
10. **Constantly late video in bunches** (RL4, M2). "Late chunks of the newest GOP are
    decode-only" and "the newest late frame is shown" overlap: when MoQ delivers late frames
    in bunches, only the last frame of each bunch is shown, so late video plays as a slideshow
    rather than "shown late". Section 12 ran it only with evenly spaced frames.
11. **Smaller inconsistencies.**
    - 6 and 9.1 say a period over 60s is not covered. LT1 keeps 120s, and LT3 / LT4 work for
      periods up to about that; 60s is the cap of `W`.
    - H7 says segments skipped by FFmpeg are stitched. FFmpeg skips after the time has passed,
      so `J` is about 0: no stitch, it is G7(b), and nothing happens (which is fine).
    - H2: playback runs out when the next segment comes later than `start` after the batch;
      `start - lower` is when its first chunk gets less than `lower`.
12. **Not in the simulation, worth adding**: delay steps that come back (1); a bufferbloat
    sawtooth (delay creeping up for tens of seconds, then dropping at once); a QUIC outage whose
    backlog comes slower than real time while the congestion window recovers, then faster (ED3
    and then ED2: two re-anchors for one outage); a sparse video-only track (2); muted
    reference audio (3); a returning track with a large first keyframe (4); a TCP stall inside
    an HLS segment (6).

Checked by reasoning and found to hold: normal flow with keyframe-sized lateness, drift up to
4% in both directions, the three forms of a stall and of a covered gap, a cellular outage whose
backlog arrives fast (G7(c), nothing left behind), an HLS encoder restart with a gap (stitched
as content, LT5 plays the next segment `lower` after it arrives, the edge follows without a
re-anchor), a bufferbloat sawtooth inside a wide band (absorbed by the band), RTMP with a late
second track, and an HLS audio rendition skewed against video.

## 15. Where the review findings went

By the session that wrote sections 1-13 (2026-11-25). Every item of 14 was checked against the
rules and against the code it cites, and items 1, 2, 5 and 6 were run in the simulation before
and after the change (`python3 design_sim.py review`). All twelve pointed at something real.

1. **Delay steps that come back**: ED3 (the window follows how long regular lateness lasts,
   and its two halves have to agree), LT7 (a rise that is taken back was lateness), LT9.
   - Before: the edge went up in every one of ten steps and the pattern never settled (MoQ
     100 / 250 / 500ms, enforce: 4 jumps while audio played, 2.0s of audio dropped, the buffer
     between 0.11s and 0.72s). After: section 12.
   - Not taken from the item: a history kept relative to the edge of its own time. It broke
     the HLS start in the simulation (10.1).
   - Two corrections to the item: the cost per step was about 0.2s of audio, not 1s, and LT3
     did see the level at the second step. What broke it was ED3 moving the edge up again.
2. **Sparse tracks**: ED3 (the window reaches back to the fourth chunk), 9.3. Before: after a
   step in delay every frame of a video-only track was late for good. After: section 12.
3. **The reference track while it pauses**: TR5. Not run, the script has no such case.
4. **A track that returns**: TR4. The rule was meant as it reads now (the track keeps its
   edge); the old wording allowed the reading of the item.
5. **An edge that never settles**: TR1, LT8.
6. **First sight after a stall inside a segment**: no rule of its own. What it leaves behind
   is taken back by DC7, and the misread is in section 6.
   - The run showed two more things, both changed: the edge moved up during the stall, from
     half a segment (ED3: both halves need chunks), and a re-anchor followed the target to
     the other side of the buffer (DC3: it ends there).
7. **Latency left inside the band**: DC7, an exception to G2 (9.1, 13).
8. **Kept content and the deadline**: RL7. The effect was smaller than the item says:
   `drop_late_groups` gives up only on groups that are still open when a newer one has
   started, after `poll_groups` handed over every frame that arrived
   `[today: track_reader.rs]`.
9. **Keyframe after a hole**: RL3, 5.3. On MoQ it holds by itself.
10. **Late video in bunches**: described in section 8 (M2); the alternative is in 13.
11. **Smaller inconsistencies**: H7 and H2 in section 8. Sections 6 and 9.1: the limit is 60s,
    and it is a rule now (`repeat_max`, LT3) instead of a side effect of the history.
12. **Scenarios to add**: the group `review` of the script. Still not run: a returning track
    with a slow first keyframe, the bufferbloat sawtooth, a backlog that comes slower than
    real time.
