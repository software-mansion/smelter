# Live sync redesign

Working document. Sections 1 and 2 have to be finalized before anything below them is filled in.

Legend: items marked `(?)` were proposed by Claude and are not confirmed yet; unmarked items came
from the user. `TBD` marks a value or decision that is still missing.

## 1. Goal

One live sync mechanism that behaves correctly and predictably for every live input protocol
(RTMP, HLS, MoQ), where each behavior is either reliably auto-detected or follows from a
characteristic of the delivery that the input passes as a hint.

- **Scenario driven.** Every scenario in section 2 has a defined expected outcome, and the design
  is checked against the whole list, not against one protocol at a time.
- **Auto-detect only what is reliable.** Some behaviors can't be told apart from timing alone. A
  behavior is auto-detected only if the detection can't misread a scenario of another protocol;
  otherwise it follows from a hint passed by the input (or from a user option).
- **Tradeoffs are written down.** Every auto-detection and every hint records what it costs
  elsewhere (e.g. detecting the HLS segment size needs long memory, which works against forgetting
  bad data quickly after a MoQ network change).
- **Live sync is protocol agnostic.** There is no protocol-layer information in live sync. An input
  can pass certain characteristics of its delivery as hints, e.g. that content might be delivered
  in segments.
  - Per-protocol constants are fine, including tuning values.
  - Information extracted from a specific stream is resolved case by case. E.g. reading the
    segment size from an HLS playlist and passing it to live sync would mix protocol matters with
    live sync too much; not a hard no.
  - `(?)` Reading: the same case-by-case rule covers runtime signals (keyframe flag, "the timeline
    was reset here", "this gap might still be filled"); each one is raised on its own in
    section 4.
  - A hint can be advisory or a contract; that is decided per hint. One that only makes sense for
    some protocols can be optional, and is a contract when defined. One that is not optional
    needs a value that can be reliably defined for every input.
- **Small public API.** The public `buffer` option (`desired_ms` / `min_ms` / `max_ms`) can change
  shape or meaning, but inputs must not end up with a bunch of different options for different
  protocols. New user-facing options are proposed with care.
  - `(?)` Reading: one option shape shared by all live inputs, as today; per protocol only the
    defaults differ (today also the floors and how missing values are derived). The one possible
    exception so far is the MoQ option of M2 and M9, if section 5 exposes it.
- **The user chooses between avoiding corrections and low latency**, through user-facing options
  that work the same on every input.
  - Required: the user can configure settings that avoid re-anchoring as much as possible.
  - Required: the user can configure settings that force low latency.
  - Nice to have, not a requirement: keeping a specific buffer target, e.g. 5s +- 0.5s, squashing
    or stretching when outside of it.
  - How it works today (context, not the requirement): with a wide `min`..`max` range the initial
    sync lands exactly on `desired` and corrections happen only in very bad circumstances; with
    `min`, `desired` and `max` close together the buffer converges on `desired`. A segment larger
    than the range is noticed and the content stays playable.
  - Examples given earlier: HLS and RTMP should hit the latency target and avoid changing the
    buffer size (stretching or squashing) when not necessary; on MoQ a 300ms target means latency
    is the most important thing, while with a 5s buffer it clearly matters much less. `(?)`
    Reading: these describe what the defaults and typical settings of each protocol should give,
    not behavior tied to the protocol.
  - `(?)` Reading: "re-anchoring" is any correction as defined in 2.1, a slew as much as a jump.
  - Which options express the two settings is a solution decision for section 5; the behavior is
    pinned down by G4, G15 and M2.
- **A/V desync only in the listed cases.** The only cases where A/V desync can be introduced:
  - the audio mixer already does it (downstream of live sync, see non-goals),
  - the sync point between the tracks is not known (the current independent mode, G14),
  - a track that is late against the reference (priority) track is shown late, and with a large
    divergence the tracks are treated as independent timelines (M2). This only happens when there
    is a reference track, which is expected only on low-latency MoQ. HLS and RTMP share one
    transport, so it is impossible there. MoQ with a large buffer should not see
    one track above `min` while the other is not on time, other than with severe network issues
    (M1), and there most general preferences do not matter either way.

Non-goals:

- Non-live paths (`SimpleSync`: HLS VOD playlists, RTMP `ingest_mode: offline`).
- RTP / WHIP / WHEP, which have their own jitter buffer (`LatencyOptimizedBuffer`). Not a goal for
  now; might be added in the future.
- Changes to the queue or to the audio resampler.
- Special support for `required` inputs or ahead-of-time processing. If some other input is
  required and blocks, live inputs rely on the buffers, and if those overflow, content is dropped.

## 2. Scenarios

"Handled" means the expected outcome is reached either by live sync on its own or with a hint the
input passes. Scenarios are grouped by protocol because that is where they are observed; live sync
itself only sees the characteristics.

A scenario describes what happens to smelter from the outside (how the arrival of packets relates
to their pts) and what is expected at the output. It does not use terms of a solution (anchor,
estimate).

Some situations are explicitly not cared about: most rules can be broken there as long as the
result is nothing too absurd. So far: a reconnect faster than the buffer (R8), a file pushed as
fast as possible (G8), HLS with `min` under 2s (H2).

### 2.1 Constraints shared by all scenarios

These come from what sits downstream of live sync and define what the outcomes can look like.

- `(?)` Terms, all observable from outside:
  - **Delivery delay** of a packet: arrival time minus pts (up to a constant). Its variation
    between packets is the jitter. A scenario describes the input by how this evolves.
  - **Playback timeline**: presentation time minus pts. Constant while nothing is corrected; a
    stretch or squash changes it gradually, a jump at once.
  - **Buffer** of a packet: presentation time minus arrival time. It differs for every packet, so
    a number given as "the buffer" has to say for which packets (e.g. the ones delivered fastest).
- Released chunks carry pts on the queue clock and the queue consumes in real time. Content mapped
  at or before "now" is late: the queue shows only the newest video frame that is due and skips
  the rest, late audio becomes a gap in the mixer.
- Every released chunk passes the decoder first. The decoder is back-pressured by the queue, so
  chunks released early wait in its input channel, out of reach of later corrections. A decoder
  that is handed a late backlog emits frames faster than real time, which shows as sped-up video.
- A correction is either a slew (audio stretched or squashed by a few percent, video frames
  repeated or dropped) or a jump (audible gap or skip, video freeze or skip).
- Video can only start or resume on a keyframe, so skipping content means skipping to a keyframe.

### 2.2 All protocols

- **G1. Start.** `(?)` Playback starts `desired` behind the newest content delivered so far; older
  content of the initial backlog is skipped.
  - Time to the first frame has no exact target. The maximum is 5s; with low latency configured
    preferably 1s, 2s is accepted. This is for normal conditions: with something weird on the
    network it can take longer.
  - It does not grow with the segment size: 20s HLS segments must not mean 20-40s before the
    first frame (H1).
  - `desired` over 5s is fine on HLS, because 3 segments are downloaded ahead of time. Worst case
    with 2s segments: about 4s of black screen in the meantime. `(?)` Arithmetic: 6s of content
    in the first burst against the default `desired` of 10s.
  - A large `desired` can take the first frame past 5s, and that is fine because the user asked
    for that buffer: RTMP with `desired` 10s shows the first frame about 9s after connect.
- **G2. Normal flow.** While the buffer does not go over the limits, the playback timeline is not
  shifted: no stretch or squash, no dropped or repeated frames.
- **G3. A/V sync.** Live sync keeps tracks with a known sync point in sync; the only exceptions are
  the ones listed in section 1.
- **G4. Slew or jump.**
  - Delivery suddenly gets later: a jump would be the best if it were known to be a new timeline,
    but the first late packets can't tell a burst that smooths out back to the old timeline from
    the start of a new one. Without that knowledge, slew.
  - The buffer is larger than needed (getting back to the old timeline): a slew is fine.
  - In both directions a jump is acceptable when the shift is too large. The shift can be a false
    positive that newer data does not confirm.
  - The threshold for a jump depends on the solution (section 5), under one constraint: something
    common must not leave the stream needing a few minutes to get back to the optimal latency. If
    abnormal states can be detected and trigger a reset on their own, the threshold does not have
    to be low; otherwise hard resets have to be more aggressive. M1 lists states that have to be
    detected.
- **G5 `(?)` Clock drift.** A source clock that differs from ours by ppm runs for hours and days
  without a jump: nothing shifts while the buffer is inside the limits (G2), and every time it
  reaches one it is slewed back (G4).
- **G6 `(?)` pts discontinuity.** Either direction, including a timestamp wrap. From the outside it
  is the delivery delay changing at once by the size of the jump, so a jump that can't be told
  apart from a change in delivery is G4. One that is known to be a new timeline starts playing
  within TBD seconds. Both tracks jumping at slightly different arrival times is one event (the
  state in between: G14), and A/V sync holds after it.
- **G7. Stall.** Delivery stops completely and resumes in one of three ways. While it is stalled
  there is too little information to tell them apart. What happens afterwards is not strict: if
  frames or audio were already dropped, keeping the old timeline makes sense, but it can also be
  treated as a newly starting connection.
  - What a burst means is known on HLS (content comes in segments, H2). On MoQ it is not known,
    so resyncing is the best bet.
  - Resyncing does not mean forgetting everything. Forgetting leads to a dangerous loop: all state
    is cleared on a stall, playback syncs to `desired`, that is not enough for a full segment, and
    content runs out again, because the timing of the previous segment was lost.
  - (a) Content continues where it stopped: the timestamps shifted relative to our clock (the
    source was paused). Leaning toward slewing to the new timeline when there is no better
    information, with a threshold above which it is a hard reset (G4).
  - (b) `(?)` Content jumps ahead by the length of the stall (the source kept running, data was
    skipped). The old timeline still fits, so keeping it needs no correction; a fresh start is
    acceptable too, as stated above.
  - (c) The missed content arrives as a burst. Ideally the packets that should already have been
    presented are dropped. It is acceptable if playback starts slowing down during the stall (e.g.
    toward 15s more at 3%), as long as it does not stay that way once the burst is done: back on
    the old timeline, or a fresh start.
- **G8. Source not real time.** Faster or slower than real time by more than a slew can absorb.
  - A file pushed as fast as possible: the behavior does not matter much. `(?)` It still must not
    grow memory without bound.
  - Slightly above what a slew absorbs, e.g. 10%:
    - Faster: slew, and at some point resync. No opinion on the threshold.
    - Slower: the most optimal would be a reset to `desired` every time playback runs out of
      content. That might not work well with other cases where content runs out, so a later
      trigger for the reset, coming from different rules, is fine.
- **G9 `(?)` Track added or removed.** While the other track plays (late RTMP config, MoQ catalog
  change). The change by itself does not shift the timeline of the playing track; it shifts only
  when the buffer no longer fits the limits afterwards (G4, G15). A new track joins in sync (G3).
- **G10 `(?)` Single track.** Audio-only and video-only inputs behave as described in this section,
  and nothing waits for a track the input does not have.
- **G11 `(?)` B-frames.** pts out of arrival order does not shift the playback timeline or the A/V
  alignment and is not read as a discontinuity.
- **G12 `(?)` End of stream.** Everything buffered is played out; the tail is not cut.
- **G14. Unknown sync point between the tracks** (the current independent mode). A/V desync can be
  introduced here. `(?)` The pts of audio and video for the same moment differ by an arbitrary
  amount, so the tracks can only be aligned by their newest content.
  - Seen only on the MoQ input, possibly a bug of the sender. MoQ now has a concept of a clock:
    when it is set the tracks are known to be synchronized. It is optional, and without it
    nothing guarantees that.
  - The intermediate state around a discontinuity on HLS (one track jumped, the other not yet) is
    a place where it could be triggered, but there can be solutions that do not treat that state
    as an unknown sync point.
  - Treating the sync point as unknown can be useful where it is not strictly necessary (M2):
    - the primary track, audio, is on time while the second one is lagging, and the user wants
      audio latency at the cost of desync;
    - on a bad network video lags more and more behind: it should not affect audio, but it is
      still nice to have at least one frame here and there.
- **G15. Delivery does not fit in the configured limits** (e.g. low latency configured with max
  400ms while the delivery delay varies by 700ms from packet to packet). Configurable and
  different per protocol:
  - Inputs that can't drop packets (RTMP; HLS technically can, but not in our setup): on a bad
    network there is no choice, the timeline shifts so that content is always on time. H2 is the
    segment case of this.
  - Inputs whose tracks have independent transport and can drop parts (MoQ): two behaviors, M2.

### 2.3 HLS

Delivery pattern:

- FFmpeg demuxes on one thread and hands out audio and video packets from the same read loop, so a
  delay always hits both tracks together. Nothing is lost or reordered.
- Content arrives in whole segments: the packets of a segment come as a burst at download speed,
  then nothing until the next segment is published and picked up by the playlist reload. Segment
  N+1 arrives about one segment duration after N, plus CDN and reload jitter.
- FFmpeg starts 3 segments before the end of a live playlist (`live_start_index=-3`), so the first
  burst holds about 3 segments of content.
- Segment duration is usually 2-10s and can be 20s or more. Live sync is not told what it is; the
  user is expected to size `buffer` for it (defaults: desired 10s, min 2s, max 20s).
- Audio and video share a timeline. `EXT-X-DISCONTINUITY` and encoder restarts change the pts base;
  MPEG-TS pts wrap every 26.5h (how FFmpeg surfaces both: to verify).
- A playlist with `EXT-X-ENDLIST` is not live and takes the non-live path.

Scenarios:

- **H1. Long segments.** With 20s segments we can't wait for multiple segments before producing:
  not 20-40s before the first frame, the target is G1.
- **H2. `desired` smaller than a segment.** The buffer option assumes the user knows the segment
  size, but e.g. `desired` of 10s with 20s segments still has to work. Artifacts at the start are
  acceptable. The problem is expected with large segments, and the flow is:
  1. The initial batch (3 segments) is downloaded.
  2. Everything except the last `desired` of it is dropped.
  3. Playback starts.
  4. It runs out of content before the next segment is received.
  5. When the next batch is received, one of:
     - playback is shifted so that the start of the new segment is now; nothing is dropped;
     - the late packets are dropped and the rest is slewed. Dropping does not make sense when it
       is already known that playback is behind, but it is acceptable if it fits the solution
       better;
     - playback is shifted so that the start of the new segment is on `desired`; no slewing is
       necessary. `(?)` Reading: the first packets of the segment are presented `desired` after
       they arrive.
  - A shift is reasonable here because the content was already interrupted.
  - HLS knows that content comes in segments, so a more optimal solution can be made than for an
    input where the meaning of a burst is unknown (G7).
  - With small segments a slew might not be enough, but that should not happen: e.g. `min` should
    never be set lower than 2s. HLS with `min` under 2s is a situation that is not cared about
    (see the top of this section).
- **H3. No large late batches.** Content that is already behind the queue is dropped instead of
  being released to the decoder, which would take a while and show as very fast sped-up video.
- **H4 `(?)` Initial backlog.** As steps 1-2 of H2 and G1, with the dropped part handled as H3.
  Where video starts when `desired` cuts into the middle of a GOP: TBD.
- **H5 `(?)` Segment arrival jitter.** A segment published or fetched late. While the buffer covers
  it nothing changes (G2). When it does not, playback runs out of content: as H2 from step 4.
  Latency added that way is removed only when it is over the limits (G4).
- **H6 `(?)` Segment duration changes mid-stream** (origin reconfigured, ad break with different
  segments). Longer segments that no longer fit in the buffer: as H2 from step 4. Shorter ones:
  nothing shifts unless the buffer is over the limits (G2, G4).
- **H7 `(?)` Origin or CDN stall.** No new segment for longer than the buffer, then several
  segments at once, or FFmpeg skips the expired ones, or the source was really paused. As G7.
- **H8 `(?)` Discontinuity** (ad insertion, encoder restart), including pts jumps under 10s. As G6.
- **H9 `(?)` Download slower than real time for a long time.** As the slower case of G8; nothing
  can be dropped in our setup (G15, R1).
- **H10 `(?)` Short segments with a small buffer** (1-2s segments, `desired` around 3s, `min` not
  under 2s as in H2) work as a lower-latency setup.
- **H11 `(?)` A/V skew inside the mux** (audio trailing video in the TS interleave, audio in a
  separate rendition with different segment boundaries) causes no A/V desync and is not treated
  as an unknown sync point between the tracks (G14).

### 2.4 RTMP

Delivery pattern:

- One TCP connection carries audio and video interleaved in dts order. Nothing is lost or
  reordered, and congestion delays both tracks equally: a stall followed by a burst.
- Delivery is continuous and real time, except right after connecting, when senders and relays
  often flush a pre-buffer or a GOP cache (hundreds of ms to several seconds).
- Timestamps are in ms, dts plus composition time (B-frames), 32-bit (wrap after 49.7 days). Both
  tracks are on one timeline.
- A track appears when its config message arrives; the two can be seconds apart.
- A new connection gets a new live sync instance and a new queue track group, after the previous
  connection ended.
- `ingest_mode: offline` takes the non-live path.
- Defaults: desired 2s, min 1s, max 5s.

Scenarios:

- **R1. Shared connection.** Everything goes over a single TCP connection and nothing can be
  dropped, so on a bad network the timeline shifts so that content is always on time (G15).
  Giving up on a late track while the other is on time (M2) is not acceptable here.
- **R2 `(?)` Startup burst.** As G1: whatever the size of the burst, it does not turn into latency.
- **R3. Stall, then a burst of backlog.** As G7(c): what is already late is dropped (H3);
  afterwards the old timeline or a fresh start.
- **R4 `(?)` Sender drops video frames under congestion** (OBS). pts gaps in video while audio is
  continuous cause no reset and no A/V desync.
- **R5 `(?)` Timestamp reset or jump mid-connection** (encoder restart behind a relay). As G6.
- **R6 `(?)` Sender not real time although `ingest_mode` is `realtime`.** As G8.
- **R7 `(?)` Config of the second track arrives late.** As G9.
- **R8. Reconnect.** A reconnect is assumed to be slower than the buffer size. Worst case when it
  is not: the queue schedules the new connection after the old one, and depending on how much
  they overlap, video is sped up while the decoder works through the buffered backlog. That
  worst case is acceptable: we don't care about this scenario, so it is fine to break most rules
  there as long as the result is nothing too absurd. `(?)` When the assumption holds, the new
  stream starts as fast as a first connection (G1).

### 2.5 MoQ

Delivery pattern:

- QUIC. Audio and video are separate subscriptions and every group (for video a GOP starting with
  a keyframe) is its own stream, so tracks and groups are delivered independently: no head-of-line
  blocking, groups can overlap or arrive out of order, and a group can be dropped by the relay or
  never complete.
- With low throughput the large track (video) falls behind while audio stays on time, and newer
  groups are typically served before old ones.
- The protocol layer knows group sequence and frame index (order, gaps, keyframes), reads all open
  groups concurrently and drops groups that can't be played anymore.
- Timestamps are pts in µs per frame, without dts.
- The catalog can carry an optional `clock`, the one continuous clock of the broadcast. When it is
  set the tracks are on one timeline; without it that is not guaranteed (G14). Our MoQ output
  sets it; the MoQ input does not read it today.
- A publisher restart or relay reconnect is a new broadcast: new live sync instance and new queue
  track group. A catalog change restarts the track of that kind inside the same instance.
- Network conditions change during a session more than for the other protocols, and the target is
  low latency. Defaults: desired 2s, min 1s, max 5s, audio prioritized.

Scenarios:

- **M1. Recovery from a severely degraded network.** Examples: throughput lower than the bitrate,
  so audio is more or less on time while video is either missing or 10-20s behind; 90-100% packet
  loss for a short or a long time. This is not exotic: it happens when driving through an area
  with bad or no coverage. Such cases have to be detected and followed by a quick clean start, so
  that within a few seconds the stream is back at a good latency with correct A/V sync. A clean
  start is a resync, which does not mean forgetting everything (G7).
  - "Recovered" is not exact: with a 200-500ms latency target, ending up at 1s is noticeable, but
    even 1-2s counts as recovered. What is left is slewed (G4).
- **M2. One track can't keep up.** With low throughput video can't be delivered in real time while
  audio is on time. Both tracks have independent transport and can drop parts, so two behaviors
  might be useful:
  - Reference track (expected only on low-latency MoQ): one track is picked as the reference. The
    other track can affect it, but the impact has to be limited: there must be some limit, and
    whether `max` is the boundary is up for discussion. A solution where the second track has no
    impact at all is not ruled out, although problems are suspected with it. Late frames of the
    other track can be dropped or shown late; shown late seems better, otherwise a consistently
    delayed track shows just one image. Today this is hardcoded for MoQ: the reference is audio
    if it exists, and video can grow the buffer only as far as shifting audio to `max`, no
    further.
  - Everything is always delivered, both tracks always on time: the same as RTMP and HLS (G15).
    Getting it is a matter of exposing one extra option in the API, inside or outside `buffer`.
    If the user gets to select the leading track, the choices are audio, video or no priority
    (M9).
  - As first stated: the buffer does not grow infinitely, and at some point the late track is
    given up if the other track is on time. Made precise later for the reference track:
    - Video that is behind is always shown, as old frames; there is no point where that stops.
      Audio that is behind is never played late.
    - At some point the tracks have to be treated as independent timelines (as with an unknown
      sync point, G14). Whether that threshold is more like 5 seconds or 5 minutes depends highly
      on the approach (section 5).
    - When the divergence is that large, it is especially important to recover fast once
      delivery improves (M1, M3).
  - Not acceptable for RTMP (R1).
- **M3. Late track becomes deliverable again.** Recovering fast is especially important when the
  divergence was large (M2). `(?)` It rejoins in sync within a few seconds, from a group start,
  without fast-forward.
- **M4 `(?)` Network gets worse** (delivery delay steps up, more jitter). As G4 while delivery still
  fits in the limits; when it does not, G15 and M2.
- **M5 `(?)` Isolated spike** (one late group, a 1s hiccup). If it pushed the buffer over the
  limits, playback is back inside them within TBD; latency added inside the limits stays (G2).
- **M6 `(?)` Group never arrives or never completes.** It is skipped at the last moment that still
  lets the next group play on time. For audio that is a short concealment, not a reset.
- **M7 `(?)` Overlapping or out-of-order groups.** Output is ordered, and a new group arriving
  before the old one ended is not read as a discontinuity.
- **M8 `(?)` Both tracks late together** (latency went up, throughput is fine). The buffer grows
  for both, as with RTMP; video is not given up.
- **M9. Audio late while video is on time.** It depends on what options are exposed to the user:
  - No controls: audio takes priority, so playback waits for audio.
  - The user selects the leading track (the reference track of M2):
    - audio: playback waits for audio;
    - video: playback waits only for video and audio is dropped. From the live sync perspective
      audio does not have to be dropped, the mixer can do it, so it might not require special
      logic;
    - no priority: playback waits for both.
  - Which of the two is exposed is an API decision for section 5.
- **M10 `(?)` Publisher restart or reconnect.** As R8.
- **M11 `(?)` Catalog change restarts one track.** As G9.
- **M12 `(?)` Joining mid-GOP.** Delivery starts at the oldest group the relay still holds that is
  within the max age of our subscription (set to `max`), each group from its start, so the first
  burst can hold about `max` of content and not only the current GOP. Playback starts within the
  G1 target.

### 2.6 Open questions

None at the moment.

## 3. Signals: what can be auto-detected and at what cost

TBD.

## 4. Hints passed by the input

TBD.

Hints mentioned so far (examples, not decisions):

- Content is delivered in segments (HLS). With it, a burst after a pause is the normal pattern;
  without it (MoQ) the meaning of a burst is unknown.
- `(?)` The tracks share one timeline: always on HLS and RTMP, on MoQ only when the catalog sets
  the optional `clock` (G14). For MoQ that is information extracted from a specific stream, so
  it falls under the case-by-case rule of section 1.

## 5. Solution

TBD.

Ideas mentioned so far (examples, not decisions):

- Inputs that can't drop packets: on every stall reset to `desired`; when just below `min`
  without stalling, slew.

## Appendix: current implementation

Reference only, as of `62d11b67`.

| | RTMP | HLS | MoQ |
| --- | --- | --- | --- |
| Buffer default (min / desired / max) | 1s / 2s / 5s | 2s / 10s / 20s | 1s / 2s / 5s |
| `track_priority` | `None` | `None` | `Audio` |
| `stabilization_period` / `_tolerance` | 500ms / 100ms | 1s / 250ms | 500ms / 100ms |
| `max_wait` | 2 x desired | 10s | 2 x desired |
| `stale_estimate_threshold` | 2s | 10s | 500ms |
| Buffer type | FIFO | FIFO | jitter buffer by (group, index) |

- Overview and known issues: `mod.rs`.
- Live edge estimate (min/max of `arrival - pts` over an adaptive window): `edge_estimator.rs`.
- Buffer range, target anchor, slew rates, track priority: `buffer.rs`.
- Start and correction decisions, shared/independent anchors: `decision_start_track.rs`,
  `decision_correct_mode.rs`, `mode.rs`; applied in `state.rs`.
- Protocol layers: `pipeline/hls/hls_input.rs`, `pipeline/rtmp/rtmp_input/connection.rs`,
  `pipeline/moq/input/{connection,track_reader,buffer}.rs`.
- Downstream: `queue/{video_track,audio_track}.rs`, `audio_mixer/input/resampler.rs`,
  `pipeline/decoder/decode_only_filter.rs`.
