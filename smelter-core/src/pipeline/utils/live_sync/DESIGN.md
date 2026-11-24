# Live sync: design proposal

A proposal for sections 3-5 of `REDESIGN.md`, written against its sections 1 and 2 as of
2026-11-24. Everything in this file was proposed by Claude and nothing is confirmed, so items are
not marked `(?)` one by one. `[today]` marks the current implementation. Scenario ids (G1, H2,
...) are the ones of `REDESIGN.md`; rule ids (ED2, DC3, ...) are defined in section 4.

1. The idea
2. Model: what is measured, what is controlled, the band
3. Structure: four stages
4. Rules, stage by stage
5. Interface: public option, input-facing API, buffer contract, stats, constants
6. Detections that can misread (section 3 of `REDESIGN.md`)
7. Hints (section 4 of `REDESIGN.md`)
8. Scenarios, one by one
9. What is given up, what differs per protocol, requirements that were not listed
10. Choices inside the design, and other designs
11. Where the rules come from
12. What was checked in a simulation
13. Open points

## 1. The idea

Every chunk has an arrival time and a pts. Their difference, the **delivery delay**, is the only
thing live sync looks at. Two numbers are measured from it, per track:

- the **edge**: the delivery delay of the fastest delivery. It says where live is;
- the **regular lateness**: how much later than the edge content arrives regularly (jitter, the
  length of an HLS segment, a stall that keeps coming back).

The playback timeline is compared with these two numbers through the limits the user configured.
Everything else follows from five rules:

1. **Two measurements, two memories.** Where live is, is believed quickly: faster delivery at
   once, slower delivery as soon as nothing else has been seen for a short while. How late
   content regularly arrives is believed only when it repeats, and it is kept for as long as it
   keeps repeating.
2. **Inside the band nothing moves.** The band is the user's limits applied to the two numbers:
   the regular slowest packets have at least the lower limit of buffer, the fastest ones at most
   the upper limit.
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
- **ED3. Slower: only when it is the whole picture.** Let `c` be the smallest delivery delay
  among the chunks that arrived in the last `W`. The edge moves up to `c` when all hold:
  - `c` is above the edge by more than `tol`;
  - delivery covered the window: it has been going on for at least `W` without an arrival gap
    longer than `0.75 x W`;
  - `c` is not still falling: the chunk that set it arrived at least `settle` ago.
  - `W = clamp(2 x L of the track, W_base, 60s)`. With bursty delivery the regular lateness
    is about one cycle long, so the window covers two cycles.
- **ED4. Settled.** The edge is settled when it has not gone down by more than `tol` in total
  during the last `settle`. A burst (delivery faster than real time) keeps the edge falling, so
  "settled" is how the end of a burst is seen.

What ED3 tells apart: after a stall or a step in delay, a backlog is delivered faster than real
time, so its delay keeps falling and ends back at the old edge: nothing moves. A new timeline
stays flat at the new level: after `W` it is the edge.

**Regular lateness.**

- **LT1. History.** The delivery delays of the last 120s, kept as the smallest and the largest
  per 250ms.
- **LT2. Occasion.** The delay *reaches* a level when a chunk arrives with at least that
  delay. Two reaches are separate occasions when delivery was clearly faster in between (under
  the level by 10% of its height above the edge, at least 10ms).
- **LT3. Twice.** A level is regular once it was reached on two separate occasions.
- **LT4. Keep.** It stays regular until twice the time between its last two occasions has
  passed since the last one, and at least for `keep_min` (10s).
- **LT5. First sight** (only with the hint "content is delivered in segments", 7). The delay
  of the first chunk after an arrival gap of at least 500ms is regular at once. It stays until
  the next such chunk replaces it, or for twice its own height above the edge (at least
  `keep_min`).
- **LT6. Value.** `L` is the highest regular level minus the edge; 0 when there is none.
- **LT7. It follows the edge.** When ED3 moves the edge up, everything in the history from
  before the window `W` moves up by the same amount. The pattern survives a shift of the
  timeline.
- **LT8. It starts with playback.** Nothing is recorded before the first burst of the track
  has ended (ED4). The initial burst is not lateness.

What the twice rule does: a late packet, a backlog after a stall, a spike, each seen once, never
make the buffer grow. Jitter repeats within a fraction of a second. An HLS segment repeats once
per segment. A stall that comes back every 20s is covered from its second occurrence on, and it
is forgotten 40s after it stopped coming.

### 4.3 Tracks

- **TR1. Which tracks count.** A track counts from the moment its edge settled for the first
  time, so once its first burst has ended, until 10s after its last chunk (`idle`). A removed
  track stops counting at once. While no track has delivered for 10s, the last values stay.
- **TR2. One timeline.** `E` and `L` of the input are combined from the tracks that count
  (2.1). There is one `T` for the input, and it is a function of the pts (DC5), so equal pts
  on two tracks are always presented at the same time.
- **TR3. A track is added.** Its chunks are scheduled on `T` from the first one, and what is
  already late is handled by RL4. It starts to count as TR1 says, so its initial burst does not
  move anything.
- **TR4. A track pauses.** Nothing waits for it. For 10s its edge and lateness still count as
  they were; after that the input is a single-track input. When the track returns it counts
  again from its first chunk.
- **TR5. Reference track** (enforce only). Audio when the input has an audio track, otherwise
  video. The other track is best effort: it is part of the target and of the band like any
  track, but only up to the cut-off of 2.4, and it never causes a jump (DC3).
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
- **DC2. Inside the band: nothing.**
- **DC3. Outside the band: re-anchor to the target.**

  | the buffer is | and | re-anchor |
  |---|---|---|
  | outside the band | nothing is playing (DC6) | jump |
  | too small | regular packets already miss their time (`B < L`) | jump |
  | too small | nothing is lost yet | slew |
  | too large | over the upper edge by at most `max_slew` | slew |
  | too large | over the upper edge by more | jump |

  - A re-anchor that has started runs until the target is reached, also after the buffer is
    back inside the band. It is re-evaluated on every tick: a slew becomes a jump when
    playback gets interrupted, and it follows the target when the target moves.
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

### 4.5 Stage 3: release (per track)

- **RL1. Order.** Chunks leave in the order of the buffer: arrival order on RTMP and HLS,
  (group, index) on MoQ (5.3).
- **RL2. On time.** A chunk is released when it is due within `ahead`. Until then it stays in
  the buffer, where a re-anchor still reaches it (2.1 of `REDESIGN.md`: a released chunk can't
  be taken back).
- **RL3. Hole** (ordered buffers only). When the next chunk in order is missing, release waits
  until the first chunk that is there is due within `ahead`, then skips the hole (M6).
- **RL4. Late.** The presentation time of the chunk has passed.
  - **Kept**, while nothing is playing (DC6) and a re-anchor to where delivery is now would
    still play it: `pts + c + B* >= now`, with `c` of ED3 (for the input: the latest `c` among
    the tracks). This is the content that continues where a stall stopped (G7(a)): when ED3
    confirms the new level, it is played from its first chunk.
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
- **RL7. Deadline for the input.** Live sync tells the input the oldest pts that can still be
  presented, `now - T` `[today: the same]`. For the best-effort video track (TR5) lateness is
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
    as delivery allows", and the buffer comes back down when the lateness is gone (DC3).
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
- **Cut.** It can drop from the front up to a pts, and it knows where the keyframes are
  (RL4). This needs a keyframe flag on a video chunk, which the chunk type does not have
  `[today: the HLS input tracks GOPs on its own]`.
- **Size.** It reports how much content it holds (RL5, stats).

It never looks at the clock and never decides that something is late: that is stage 3.

### 5.4 Stats and events

No internal terms in the names (the edge is "live", the playback timeline is an offset):

- gauges per input: `buffer_ms` (`B`), `target_buffer_ms` (`B*`), `regular_lateness_ms`
  (`L`), `buffer_min_ms` and `buffer_max_ms` (the band), the state (waiting, playing,
  interrupted, re-anchoring);
- counters: re-anchors by kind (start, jump, slew) and by reason (buffer too small, buffer too
  large, content late, interrupted), late content dropped per track (ms), late video shown,
  discontinuities stitched and undone, time with nothing playing;
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

Shared: memory of the lateness 120s at 250ms resolution, `keep_min` 10s, gap for first sight
500ms, `idle` 10s, capacity 120s, `ahead` 500ms, slew rate 1-4%, `W` at most 60s, stitch
thresholds 10s forward and 1s backward, pair window 2s, unknown sync point 5s / 3s.

- `ahead` trades the reach of a re-anchor against the time the decoder gets
  `[today: released up to 5s ahead]`. With a buffer under `ahead` everything is released on
  arrival.

## 6. Detections that can misread

What section 3 of `REDESIGN.md` asks for: every detection that can be wrong, the state a wrong
reading leads into, and how it is left. Six of them; "nothing is playing" (DC6) and the limits
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
  - Misread 1: a backlog that is delivered at exactly real-time speed for longer than `W`
    (a sender that can't send faster). It is taken as a new timeline; for the viewer it is
    one. If it speeds up later, the edge falls at once (ED2), the buffer is too large by the
    shift, and DC3 removes it or it stays inside the band.
  - Misread 2: a real shift is confirmed `W` late. Content of that time is kept (RL4), so
    nothing is lost; the freeze is longer by `W`.
  - Cost: `W` (1s; on HLS two segments) before slower delivery is believed.
- **Regular or one-off (LT3, LT4).**
  - Unlocks: one-off problems add no latency (G4, M5), periodic ones are covered (M3, H2).
  - Misread 1: two unrelated stalls of similar size inside 60s count as regular. The buffer
    grows by the size of the stall, by a slew, and stays that way for up to twice their
    distance; then DC3 brings it back if it is over the band.
  - Misread 2: a problem with a period over 60s is one-off every time, and every occurrence
    is late content.
  - Misread 3: the first occurrence of something periodic is always treated as one-off. Cost:
    one interruption more than a design that grows the buffer on the first stall (10.1).
  - Cost: after the conditions get better, the lateness is remembered for at least `keep_min`.
- **First sight (LT5), with the hint.**
  - Unlocks: with segments longer than the starting buffer, playback is interrupted once, not
    twice (H2, H6), and content after a paused source plays at once (H7).
  - Misread: several segments at once after an origin stall are taken as the new normal. The
    backlog is played late, and at the next regular segment the buffer is over the band and
    DC3 cuts it (a skip). In the simulation: 20s of old content played, then a 40s skip.
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
  (section 12: 11.4s instead of 5.9s with nothing playing, for 20s segments on a 10s starting
  buffer).
  - Cost elsewhere: none on other protocols. On HLS: the fourth misread of section 6.
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
  nothing to take back. A periodic problem is covered from its second occurrence (LT3, LT4). A
  new timeline against a burst: ED3.
- **G5 Clock drift.** The edge follows it (ED2 at once, ED3 in steps of `tol`). Nothing moves
  inside the band; at an edge of the band the buffer is slewed back to the target.
- **G6 pts discontinuity.** Stitched (4.1): no artifact, nothing forgotten. Not stitched (under
  the thresholds, or MoQ): forward, the edge falls at once and the buffer is larger by the
  jump (inside the band: a hole and that much latency; over it: DC3). Backward: as G7(a).
- **G7 Stall.**
  - (a) Content continues: it is late, so it is kept (RL4). ED3 confirms the new level after
    `W`, DC3 jumps, and playback goes on from the first chunk after the stall.
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
  the timeline moves only if the buffer of the remaining track is outside the band.

HLS:

- **H1.** DC1 does not wait for a segment: the first frame comes `settle` after the batch.
- **H2.** Playback runs out when the next segment comes later than `start - lower` after the
  batch. Its first chunk is regular at first sight (LT5), nothing is playing, so DC3 jumps and
  the segment plays from its start, `lower` after it arrived. One interruption.
- **H3.** RL4.
- **H4.** The buffer lands between `start` and `start` plus one segment, depending on when the
  next segment is published. That is inside the band, so nothing is re-anchored.
- **H5.** Late segments are covered by `lower`, and by the lateness that repeats (LT3). One
  that is later than that: as H2.
- **H6.** Longer: as H2, one interruption. Shorter: the lateness shrinks after LT4, and the
  buffer is re-anchored only if it is over the band then.
- **H7.** Source paused: as H2, playback resumes `lower` after the first segment; the edge
  follows later, without a re-anchor. Several segments at once: the fourth misread of
  section 6. Segments skipped by FFmpeg: a forward pts jump, stitched (G6).
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
  shown late. Everything lost for a while: nothing is playing, and on resume G7.
- **M2.** enforce is the reference track of M2 (TR5). Late video is shown (RL4), late audio is
  dropped. extend is "everything is always delivered".
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

- **Latency added by a periodic problem stays when the problem ends, if it is inside the
  limits** (G4, M5). G2 wins: inside the band nothing moves. It is taken back only when the
  buffer is over the band.
  - Why: taking it back needs a second target inside the band and a second set of thresholds.
  - What would change it: 13.
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
- **A periodic problem with a period over 60s is not covered** (M3).
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
  re-anchor that runs to the target, and LT4 against shrinking in every good period.
- **Bounded memory** (RL5), also when a source never stops bursting.
- **A broken timestamp** moves nothing by itself (ED2), and on RTMP and HLS it is stitched
  away and back.
- **Explainable.** Every re-anchor has one reason out of a short list (5.4).
- **A/V sync through a slew.** Both tracks read one function of the pts (DC5).
- **Sparse tracks** (a frame every few seconds) need no special case: between frames nothing is
  playing, so a re-anchor there is free and invisible.

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
- **Growing never jumps while nothing is lost; shrinking jumps only past `max_slew` over the
  band.** A jump to grow is a freeze for nothing. A source that runs 3% fast reaches the
  upper edge again and again, and has to be slewed back each time (G5).
- **No re-centering while nothing is playing.** It would be free, but it shrinks the buffer
  right after an interruption, which made the next one longer in the simulation.
- **The window of ED3 follows the regular lateness** instead of a separate measurement of
  arrival gaps `[today: 4 x the longest gap of the last 60s]`: one estimator less.

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

## 11. Where the rules come from

Each of these was read in the source or the documentation of the project while preparing this
proposal (local clones of libwebrtc, FFmpeg, hls.js, OBS and moq; the rest online).

- **The edge as a minimum over a short window (ED2, ED3).**
  - NetEq: delay relative to the fastest packet of the last 2s (`packet_arrival_history.cc`).
  - GStreamer `rtpjitterbuffer`: minimum over 2s or 512 packets (`calculate_skew`).
  - MoQ `hang` player, current main: the reference moves later only when every arrival in a
    2s window trails it (`js/watch/src/sync.ts`, `doc/concept/audio-jitter.md`). ED3 is this
    rule plus "not still falling".
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

`design_sim.py` (next to this file) is a model of the rules in about 600 lines of Python: chunks
are (arrival, pts) pairs, there is no media, decoder or queue. It was used to find rules that
did not work, not to tune values. `python3 design_sim.py [rtmp] [hls] [moq] [av] [misc]`.

Not modeled: two-track stitching (ST3, ST4), TR6, keyframes and decode-only (RL4), holes (RL3),
capacity (RL5), and DC5 (a re-anchor is applied to all unreleased content at once).

Results of the last complete run. One rule changed after it and was not run again: DC3 cuts a
buffer that is too large when it is `max_slew` over the band; in the run it was `max_slew` from
the target. This affects the two lines marked with *.

RTMP, 1s / 2s / 5s:

- 3s burst at the start: first frame after about 1s, buffer 2.00s from then on.
- 5s stall, backlog at 5x: no re-anchor; 4.2s of the backlog dropped; buffer 2.00s.
- 5s stall, content continues: re-anchored 1.0s after delivery resumed; nothing dropped.
- 5s stall, content skipped; 1.5s gap with a burst after it: nothing.
- 0.6s gap, content continues: buffer 1.40s, no re-anchor. 1.5s gap: buffer 0.50s, slewed
  back to 2.00s in 44s.
- 2s stall with a backlog every 20s: nothing on the first one, a slew to 3.00s from the
  second; nothing is late after that. After the last one the buffer stays at 3.00s (9.1).
- pts jump of 30s, either direction: stitched, buffer unchanged. Without stitching: one jump,
  about 1.7s with nothing playing.
- Source 0.1% fast: no re-anchor in 240s. 3% slow: a steady 3% slew, buffer about 1.8s. 10%
  slow: playback runs out and jumps back about every 27s.
- * Source 3% fast: a 3.1s skip every 100s in the run; with the changed rule it should be
  slewed back at the upper limit. * 10% fast: a 3.1s skip every 31s.

HLS, 2s / 10s / 20s:

- 6s segments: first frame at 2.8s (1.8s of download); buffer 11.2s or 14.2s depending on
  when the next segment comes; no re-anchor. The same with 0-1.5s of arrival jitter.
- 20s segments, next segment 20s after the start: one interruption (5.9s with nothing
  playing), then buffer 20s. Next segment 8s after the start: no interruption, buffer 26s.
- The same without the segments hint: two interruptions, 11.4s.
- 2s segments on 2s / 3s / 6s: no re-anchor, buffer 3.9s.
- Segments change from 6s to 20s: one interruption.
- Source paused for 60s: playback resumes 2s after the first segment, nothing dropped.
- Origin stall, three segments at once: the backlog plays 40s late; 20s later a 40s skip.
- pts jump at a segment boundary: stitched, no artifact. Without stitching: one jump, 9.7s
  with nothing playing.

MoQ, 100 / 250 / 500ms:

- Delays of up to 400ms start: the buffer is slewed from 0.25s to 0.45s; no jump. When they
  stop it stays at 0.45s (inside the limits).
- One 1s hiccup: no re-anchor; 1.1s lost.
- A 0.6s hiccup every 15s, extend: buffer 0.70s from the second one; slewed back to 0.25s
  30s after the last one. enforce: 0.50s, and about 0.1s lost per hiccup.
- Video delivered at 70% of real time for 30s, audio on time. enforce: audio is not touched
  apart from a slew to 0.50s; video is shown late for 42s and is in sync again a few seconds
  after it recovers. extend: 39 jumps in 30s, audio included.
- Audio received 0.8s before video, enforce: video is always shown late, about 0.3s behind.
- Audio pauses for 30s while video plays (RTMP limits): no re-anchor.

## 13. Open points

- **The public option.** The name `max_policy`, `enforce` as the MoQ default, and whether the
  leading track should be selectable (M9).
- **Latency that a periodic problem added, after it ends** (9.1). The alternative: when the
  regular lateness has shrunk and the buffer is above the target by more than some margin,
  slew back, although it is inside the band. It breaks G2 a little.
- **extend on MoQ with one track slower than real time** is unusable while it lasts (39 jumps
  in the simulation). With a shared clock nothing takes the tracks apart (TR6 is off).
  Options: leave it (the user chose "everything is delivered"), or let TR6 apply in extend
  whatever the hint says.
- **The keyframe flag** (RL4, 5.3): on the chunk type, or the cut stays in the inputs
  `[today: HLS only]`.
- **`ahead`** (500ms): how much time the decoder really needs was not measured.
- **Several HLS segments at once** (9.1): play late and skip, or cap first sight (for example
  at twice the regular lateness) and accept a longer freeze after a paused source.
