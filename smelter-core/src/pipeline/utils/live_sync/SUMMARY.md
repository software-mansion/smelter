# Live sync: how it works

Live sync decides when each chunk of a live input (RTMP, HLS, MoQ) is played. This document
explains the ideas behind it and what happens in the common situations. The complete list of
rules, with the ids that the code comments refer to (`ED3`, `LT4`, `DC7`, ...), is in
`DESIGN.md`.

## TL;DR

- Live sync answers one question per chunk: **when is it presented?** The answer is always
  `pts + T`. `T` is the **playback timeline**, one for the whole input, so audio and video with
  the same pts are presented at the same time.
- It looks at one signal only: the **delivery delay** of a chunk, `arrival - pts`. Two things
  are measured from it:
  - the **live edge**: the fastest delivery, which says where live is;
  - the **regular lateness**: how much later than the edge content arrives on a regular basis.
- The distance between playback and the live edge is the **buffer**. It starts at `start` and is
  left alone inside a **band**: the regular slowest content still has `lower` of it, the fastest
  has at most `upper`.
- When the buffer leaves the band, the timeline is **re-anchored**: moved so that the buffer is
  back at its target. Gradually (a **slew**, content plays 1-4% faster or slower) when nothing
  is being lost, at once (a **jump**) when nothing is playing or content is being lost anyway.
- **A single late chunk never moves the timeline.** It is simply late. The timeline moves only
  when the measurements move, and those need slower delivery to last or lateness to repeat.
- Start, stall, timestamp jump, a track that comes or goes: none of them is a separate mode.
  They are the same rules in a moment when nothing is playing or a track is not counted yet.

`start`, `lower` and `upper` are the `desired_ms`, `min_ms` and `max_ms` of the input's `buffer`
option.

## Core ideas

### One signal: the delivery delay

Every chunk has a pts and a time at which it arrived. Their difference is the delivery delay.
Its absolute value means nothing, because the pts of a stream start anywhere. What matters is
how it compares between chunks:

- a chunk with a smaller delay than any before it was delivered faster: live is at least there;
- a chunk with a larger delay was held up somewhere on the way.

Nothing else is read from a chunk. There is no protocol-specific timing logic; a protocol only
provides options and two hints (see [Per protocol](#per-protocol)).

### Two measurements, two kinds of memory

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

**The live edge `E` is believed quickly.** Content can't arrive before it exists, so the
smallest delay seen so far is where live is.

- Faster delivery moves the edge at once.
- Slower delivery moves it only after it has lasted: a whole window of chunks is later than the
  edge, and delivery is not catching up within that window. The window is at least 1s (HLS: 2s)
  and grows to twice as long as lateness regularly lasts, so a stall that is known to come and
  go is not mistaken for a new edge.
- A rise is taken back when delivery returns to the old edge right after it. It was lateness
  after all.

**The regular lateness `L` is believed only when it repeats.** A level of delay counts as
regular once it was reached on two separate occasions at most 60s apart (jitter, the length of
an HLS segment, a stall every 20 seconds). It stays regular for twice the longest time between
two occasions, at least 10s, and is forgotten after that. Delays of the last 120s are remembered
for this.

- One late chunk or one burst is one occasion: it is not regular and changes nothing.
- Lateness is remembered relative to the edge. When the edge moves, what was learned moves with
  it.
- One shortcut, for inputs that say their content is delivered in segments (HLS): the first
  chunk after a pause in delivery counts right away. It shows how late delivery gets within a
  segment, without waiting for a second one.

### One control: the playback timeline

A chunk is presented at `pts + T`. Everything live sync does to playback, it does by choosing
`T`:

- the **buffer** `B = T - E` is how far playback is behind live, so the latency live sync adds;
- `B - L` is the buffer of the content that is regularly the slowest. If it is negative, content
  is late on a regular basis.

### Target and band

The three limits turn the two measurements into a target for the buffer and a band around it:

```
target   B* = max(start, L + lower)
band     L + lower  <=  B  <=  upper + (B* - start)
```

- While the regular lateness fits under the starting buffer, the target is `start` and the band
  is `L + lower .. upper`.
- When it does not fit, the whole window moves up by what is missing, and the target is the
  smallest buffer that still leaves `lower` for the regular slowest content.

| limits (lower / start / upper) | regular lateness | target | band |
|---|---|---|---|
| 1s / 2s / 5s | 50ms of jitter | 2s | 1.05s .. 5s |
| 1s / 2s / 5s | a 2s stall every 20s | 3s | 3s .. 6s |
| 2s / 10s / 20s | 6s HLS segments (5.4s) | 10s | 7.4s .. 20s |
| 2s / 10s / 20s | 20s HLS segments (18s) | 20s | 20s .. 30s |

`lower` and `upper` can't always both hold. What happens then is a choice:

- **extend**: `lower` wins. The buffer grows past `upper` as far as the regular lateness needs,
  as in the table above. Nothing is lost.
- **enforce**: `upper` wins. Playback is never more than `upper` behind the live edge of the
  reference track (audio while it delivers). Content that can't arrive in that time is late.

### Re-anchoring: jump or slew

A re-anchor is any change of `T`. It always aims at the target, not just back inside the band,
and it is as abrupt as what the viewer already sees:

| the buffer is | and | re-anchor |
|---|---|---|
| outside the band | nothing is playing | jump |
| too small | regular content already misses its time | jump |
| too small | nothing is lost yet | slew |
| too large | by at most `max_slew` (1.2s, HLS: 2.4s) | slew |
| too large | by more | jump |

- A slew changes `T` by 1-4% of the content that is played: a small difference takes about 5s,
  one second takes 25s. It runs until the target is reached and follows the target if it moves.
- A slew to later turns into a jump when playback gets interrupted in the meantime.
- **What live sync added, it takes back.** Inside the band nothing moves, with one exception:
  latency that a re-anchor added is slewed back once its reason is gone (the lateness is no
  longer regular, or the rise of the edge was taken back). Latency that the stream brought
  itself stays.

### A change applies to content that was not released yet

`T` is a function of the pts, not a single number that changes in time.

```
  T
  ^            jump                          slew
  |   ---------+
  |            |                                  ________  target
  |            +-----------            __________/
  +------------^----------------------^--------------------->  pts
           frontier               frontier
```

A re-anchor changes `T` only for content after the **frontier**: the newest pts that any track
has already released. A jump is a step there, a slew is a ramp from there on. Both tracks read
the same function, so equal pts are presented at the same time even while a re-anchor is in
progress, and no track has to wait for the other one.

## Four stages

Every written chunk and a 20ms ticker run the same four stages in order, under one lock per
input.

```
 input --chunk--> 0 STITCH --> 1 MEASURE --> 2 DECIDE --> 3 RELEASE --> decoder --> queue
```

| stage | owns | does not |
|---|---|---|
| 0 Stitch | the pts offset that hides timestamp jumps | touch timing |
| 1 Measure | edge and regular lateness of each track | know what is playing |
| 2 Decide | the playback timeline | see single chunks |
| 3 Release | the buffer of each track | change the timeline |

The separation is what keeps a late chunk from moving the timeline: the stage that sees chunks
can't change `T`, and the stage that changes `T` sees only the two measurements.

### Stage 0: stitch

When the timestamps of a stream jump (back by more than 1s, or ahead by more than 10s beyond the
time that passed), the content after the jump gets an offset so that it continues where the
content before it ended. The delivery delay stays continuous and the later stages notice
nothing.

- Tracks jump one after another. The first one waits; when the other track jumps the same way,
  it takes over the same offset, so the relation between audio and video is kept.
- If the other track delivers 2s of content without jumping, it was a gap in one track and not
  a new timeline. The offset is undone.
- The sink gets a discontinuity in front of the first chunk after a jump.
- It needs chunks in the order they were written, so it is on for RTMP and HLS and off for MoQ.

### Stage 1: measure

Per track, from its first chunk on: the edge and the regular lateness described above.

- A track **counts** once its first burst has ended (its edge stopped falling for `settle`).
  Before that it does not take part in the measurements of the input, so the burst a track
  starts with moves nothing.
- A track that delivered nothing for 10s stops counting until it delivers again. Nothing waits
  for it.

### Stage 2: decide

Per playback timeline. The tracks on it are combined: the edge of the input is the latest edge
among them (the earliest moment at which the content of every track is there), the lateness is
the one of the slowest track. From these it computes the target and the band, and re-anchors by
the table above.

### Stage 3: release

Per track. Chunks leave in the order of the buffer.

- **On time.** A chunk is released 500ms before its presentation time. Until then it stays in
  the buffer, where a re-anchor still reaches it. A released chunk can't be taken back.
- **Late.** Its presentation time has passed.
  - Audio is dropped.
  - Video is cut back to its newest keyframe. Older late GOPs are dropped; the rest is given to
    the decoder without being shown, so it is ready for the first frame that is on time. If
    nothing on time follows, the newest late frame is shown.
- **Kept.** While nothing is playing, late content is not dropped as long as a re-anchor to
  where delivery is now would still play it. This is how playback continues where it stopped
  after a stall.
- **Capacity.** A track holds at most 120s of content; beyond that the oldest is dropped.
- **End of stream.** Everything that is left is released.
- The input can ask for the oldest pts that would still be played, and skip reading content
  that is older (MoQ gives up on such groups).

## Tracks

- **One timeline per input.** Audio and video share `T`, so they stay in sync by construction.
- **A track is added later.** Its chunks are scheduled on the existing timeline from the first
  one, and what is already late is handled as late. Without the hint below, it is scheduled
  once it counts: only then its edge shows which timeline it belongs on.
- **A track pauses** (a muted microphone). For 10s its measurements still count as they were,
  then the input behaves like a single-track input. When it returns, it counts again at once.
- **Reference track** (enforce only). Audio while it counts, otherwise video. The other track
  is best effort: it takes part in the target up to the cut-off, and its late content never
  causes a jump.
- **Unrelated timestamps.** An input can say that its tracks share one timeline (RTMP and HLS
  always, MoQ when the catalog says so). Without that hint, tracks whose edges are 5s or more
  apart are synchronized separately: video gets its own timeline with its own target, band and
  re-anchoring. When the edges come within 3s, video is re-anchored to the timeline of audio.

## What happens when...

**A stream starts.** The first chunks usually arrive in a burst, each one "faster" than the one
before, so the edge keeps falling. When it has not fallen for `settle` on every track that has
delivered something, the burst is over and the edge is where live is. Playback starts with
`T = E + start`. Whatever the burst held beyond `start` is late at that moment, so playback
begins `start` behind live and not further. If the edge never settles (a source faster than real
time), playback starts `start_timeout` after the first chunk.

**Delivery has jitter.** Jitter is lateness that repeats, so it becomes `L`. With 50ms of jitter
and limits 1s / 2s / 5s the band is 1.05s .. 5s and the buffer sits at 2s. Nothing moves.

**One chunk is late, or delivery stalls once.** The measurements do not change: it happened
once. Chunks that miss their time are late. If the stall is longer than the buffer, playback is
interrupted. When delivery catches up, the content that missed its time is skipped and playback
continues with what is on time.

**A stall keeps coming back.** After the second one the level is regular. With limits
1s / 2s / 5s and a 1.5s stall every 20s, the target becomes 2.5s and the band 2.5s .. 5.5s. The
buffer of 2s is too small, but the stalled content still arrives in time, so the timeline slews
to later. With 3s stalls the content would already miss its time, and it would be a jump. When
the stalls stop, the level stays regular for another 40s. Then the target is 2s again, and the
half second that live sync added is slewed back.

**Delivery gets slower and stays there** (the source was paused, or the network path changed).

- If content still arrives in time, playback goes on. After a window of slower delivery the edge
  rises, and the buffer is smaller by that much. Inside the band nothing happens; outside it is
  a slew.
- If content arrives too late, playback is interrupted. As soon as the measurements show that
  the buffer is outside the band, the timeline jumps, which costs nothing while nothing is
  playing. On HLS that is right away: content is delivered in segments, so the first chunk after
  a pause in delivery already shows how late it gets. Content that arrived in the meantime was
  kept, so playback continues where it stopped.

**The timestamps of the stream jump** (RTMP, HLS). Stage 0 hides it. Playback continues without
a gap, and the decoder is told about the discontinuity.

**Video is slower than audio** (MoQ, where tracks are delivered separately).

- enforce: playback stays at most `upper` behind the live edge of audio. Audio plays without
  gaps. Video that can't make it is shown late, as well as it can be.
- extend: the buffer grows until video makes it too. Latency goes up instead of video being
  late.

## Per protocol

A protocol contributes limits, a few constants and two hints. The rules are the same.

| | RTMP | HLS | MoQ |
|---|---|---|---|
| default limits (lower / start / upper) | 1s / 2s / 5s | 2s / 10s / 20s | 1s / 2s / 5s |
| `settle`: the edge stopped falling | 500ms | 1s | 500ms |
| tolerance of comparisons | min(100ms, start/10) | min(250ms, start/10) | min(100ms, start/10) |
| shortest window for a rise of the edge | 1s | 2s | 1s |
| `start_timeout` | max(2 x start, 3s) | 30s | max(2 x start, 3s) |
| `max_slew` | 1.2s | 2.4s | 1.2s |
| stitch | on | on | off |
| hint: delivered in segments | no | yes | no |
| hint: tracks share one timeline | yes | yes | when the catalog has `clock` |
| when `lower` and `upper` clash | extend | extend | `max_policy`, default enforce |

## What can be observed

Stats are reported per track.

- `state`: `waiting_for_start`, `playing`, `interrupted` (the track has nothing to play) or
  `reanchoring` (a slew is in progress).
- `buffer_seconds`, `target_buffer_seconds`, `buffer_min_seconds`, `buffer_max_seconds` and
  `regular_lateness_seconds`: `B`, `B*`, the band and `L`.
- `reanchors`: counts by kind (`jumps`, `slews`) and by reason (`buffer_too_small`,
  `buffer_too_large`, `content_late`, `interrupted`, `added_latency_taken_back`,
  `tracks_joined`).
- `chunks_dropped`, `late_chunks_shown`, `interrupted_seconds`, `discontinuities_detected`.

Every re-anchor is also logged once, with its reason and the numbers it was decided on.

## Where things are

| file | what |
|---|---|
| `mod.rs` | options, `LiveSync`, the ticker |
| `state.rs` | the state of an input, runs the stages, tracks and their timelines |
| `stitch.rs` | stage 0 |
| `edge.rs`, `lateness.rs`, `measure.rs` | stage 1 |
| `decide.rs`, `timeline.rs` | stage 2: target, band, re-anchors; `T` as a function of the pts |
| `release.rs` | stage 3 |
| `buffer.rs`, `track.rs` | the buffer contract, the write handle of a track |
| `stats.rs` | what is reported |
| `design_sim.py` | a model of stages 1 and 2 with scenarios |
