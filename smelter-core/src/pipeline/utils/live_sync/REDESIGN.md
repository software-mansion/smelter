# Live sync redesign

Working document. Sections 1 and 2 have to be finalized before anything below them is filled in.

Legend: items marked `(?)` were proposed by Claude and are not confirmed yet; unmarked items came
from the user. `TBD` marks a value or decision that is still missing. `[today]` and
`[today: ...]` describe the current implementation: context, not a requirement. The appendix is
all about the current implementation and is not marked.

## Terms

- **Playback timeline**: timestamps of the stream are mapped to timestamps of the queue using a
  single offset. That offset is the playback timeline.
- **Re-anchoring**: whenever the playback timeline changes.
  - A **slew** changes it gradually, with content stretched or squashed by a few percent; a
    **jump** changes it at once.
- **Buffer** of a packet: presentation time minus arrival time. It differs for every packet, so a
  number given as "the buffer" has to say for which packets (e.g. the ones delivered fastest).

## 1. Goal

One live sync mechanism that behaves correctly and predictably for every live input protocol
(RTMP, HLS, MoQ), where each behavior is either reliably auto-detected or follows from a
characteristic of the delivery that the input passes as a hint.

- **Scenario driven.** Every scenario in section 2 has a defined expected outcome, and the design
  is checked against the whole list, not against one protocol at a time.
  - The scenarios are examples of expected behavior, not a complete specification. The design
    extrapolates them to every other situation.
  - Only real conflicts between requirements are discussed, with specifics (2.6).
  - If no reasonable solution covers all of them, the least important requirements give way.
    Simplicity can be more important than handling some of the cases.
  - Every requirement that the solution abandons is listed in section 5, each with the reasoning
    for giving it up.
- **Unreliable detection is a cost.** Some behaviors can't be told apart from timing alone. A
  detection that can misread a scenario is acceptable when it unlocks important functionality;
  where it does not, the behavior follows from a hint passed by the input (or from a user
  option).
  - It is never spent on something unimportant, not even once. For very important functionality
    it is acceptable, but not if the design ends up with a lot of such detections.
  - Every use comes with all of its failure cases considered: what state each one leads into.
  - A failure case can be judged unimportant (e.g. the network is so bad that no one can expect
    good quality), but then it also has to be shown that the state is recoverable once the
    conditions change, and how quickly.
- **Tradeoffs are written down.** Every auto-detection and every hint records what it costs
  elsewhere (e.g. detecting the HLS segment size needs long memory, which works against forgetting
  bad data quickly after a MoQ network change).
- **Live sync is protocol agnostic.** There is no protocol-layer information in live sync. An input
  can pass certain characteristics of its delivery as hints, e.g. that content might be delivered
  in segments.
  - Per-protocol constants are fine, including tuning values.
  - A timestamp wrap, or anything else where a value reaches its limit and rolls over to zero, is
    handled on the protocol level, before live sync.
  - Arrival order matters only for some protocols. A jitter buffer for the protocols that need
    one is the responsibility of live sync `[today: MoQ only]`. Its implementation can be per
    protocol (at least for some), but it still counts as part of live sync, because there has to
    be a common contract between live sync and the buffer.
    - `[today]` The contract is the `LiveSyncBuffer` trait. RTMP and HLS use a FIFO; the MoQ
      input brings a jitter buffer ordered by (group, index).
  - Information extracted from a specific stream is resolved case by case: passing it to live
    sync would mix protocol matters with live sync too much; not a hard no. Examples: the segment
    size read from an HLS playlist, a keyframe flag, "the timeline was reset here",
    "this gap might still be filled".
  - Communication in the other direction, from live sync to the input, is avoided. It is fine
    where it is needed, as long as it does not become too complex.
    - `[today]` Live sync gives the input the oldest pts that can still be presented, and the
      MoQ input stops reading groups that are behind it.
  - A hint can be advisory or a contract; that is decided per hint. One that only makes sense for
    some protocols can be optional, and is a contract when defined. One that is not optional
    needs a value that can be reliably defined for every input.
- **Small public API.** The public `buffer` option `[today: desired_ms / min_ms / max_ms]` can
  change shape or meaning, but inputs must not end up with a bunch of different options for
  different protocols. New user-facing options are proposed with care.
  - An option can differ between protocols when there is a good reason (e.g. the MoQ option of
    M2 and M9). Every such difference has to be justified in section 5.
    - `[today]` One option shape shared by all live inputs; the defaults, the floors and how
      missing values are derived differ per protocol.
  - Public API options do not have to be the same, one to one, as the options of live sync.
    - `[today]` The input resolves `buffer` into `min` / `desired` / `max` and adds per-protocol
      values the user never sees (track priority, stabilization period and tolerance, max wait,
      stale estimate threshold).
- **The user chooses between avoiding re-anchoring and low latency**, through user-facing options.
  - Required: the user can configure settings that avoid re-anchoring as much as possible. It can
    be expressed as a specific option or just as the user setting a larger buffer value.
  - Required: the user can configure settings that force low latency. It can be expressed as a
    specific option or just as the user setting a smaller buffer value.
  - Not every one of these is necessary on every protocol: e.g. low latency is not expected from
    HLS.
  - Nice to have, not a requirement: keeping a specific buffer target, e.g. 5s +- 0.5s, squashing
    or stretching when outside of it.
  - `[today]` With a wide `min`..`max` range the initial sync lands exactly on `desired` and
    re-anchoring happens only in very bad circumstances; with `min`, `desired` and `max` close
    together the buffer converges on `desired`. A segment somewhat larger than the range is
    noticed and the content stays playable; a much larger one: problems, memory of the edge
    estimate.
  - Examples given earlier: HLS and RTMP should hit the latency target and avoid changing the
    buffer size (stretching or squashing) when not necessary; on MoQ a 300ms target means latency
    is the most important thing, while with a 5s buffer it clearly matters much less.
    - Live sync can't know about protocols, so `[today: it relies on the buffer values]`. That is
      not a hard rule: the logic does not have to be uniform, the same option value on HLS and
      on MoQ does not have to lead to the same behavior.
    - The protocol alone is not enough information either: MoQ can be both low latency and large
      buffer.
  - Which options express the two settings is a solution decision for section 5; the behavior is
    pinned down by G4, G15 and M2.
- **A/V desync only in the listed cases.** The only cases where A/V desync can be introduced:
  - the audio mixer already does it (downstream of live sync, see non-goals),
  - the sync point between the tracks is not known (G14) `[today: independent mode]`. Which
    cases count is not strict: tracks that start from unrelated timestamps, maybe also tracks
    that started together and diverged with time,
  - one track can't be on time together with the reference (priority) track inside the
    configured limit (M2).
    - Example: a hard maximum buffer of 500ms while audio is received 1s earlier than video.
    - Outcome: the late track is shown late. With a large divergence the tracks are treated as
      independent timelines.
    - It needs a reference track, which is expected only on low-latency MoQ.
    - HLS and RTMP: impossible, the tracks share one transport.
    - MoQ with a large buffer: one track above the lower limit `[today: min]` while the other is
      not on time should not happen, other than with severe network issues (M1), and there most
      general preferences do not matter either way.

Non-goals:

- Non-live paths `[today: SimpleSync]`: HLS VOD playlists, RTMP `ingest_mode: offline`.
- RTP / WHIP / WHEP, which have their own jitter buffer `[today: LatencyOptimizedBuffer]`. Not a
  goal for now; might be added in the future.
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

Scenarios do not assume that today's options stay (section 1, small public API). Where a scenario
needs one, it is marked: the starting buffer `[today: desired]`, the limits `[today: min, max]`.
The final options do not have to have a counterpart for each.

Some situations are explicitly not cared about: most rules can be broken there as long as the
result is nothing too absurd. So far: a reconnect faster than the buffer (R8), a file pushed as
fast as possible (G8), HLS with a lower limit under 2s `[today: min]` (H2).

### 2.1 Constraints shared by all scenarios

Two lists: how the elements downstream of live sync behave, and what that means for live sync.

Downstream behavior (decoder, queue, audio mixer):

- **Decoder.** Every released chunk passes it first.
  - It is back-pressured by the queue, so chunks released early wait in its input channel.
  - Decoding takes time, and it can only start on a keyframe.
- **Queue.** It consumes in real time, by the pts that released chunks carry on the queue clock.
  Content with a pts at or before "now" is late.
  - For every output frame it shows the newest video frame that is due and skips the older ones.
    A late frame is still shown while there is nothing newer.
  - With a backlog of late video it skips through the frames as fast as the decoder emits them,
    which shows as sped-up video.
  - During a gap in video it keeps showing the old frame for as long as the input tells it to,
    through an API it already has (the stale frame timeout).
    - `[today]` Every input type passes one fixed value. RTMP and HLS set the timeout: the frame
      is shown until it is 3s old (the default), then the input shows nothing. MoQ does not set
      it: the last frame stays.
- **Audio mixer.** It drops chunks that are in the past.

Constraints on live sync:

- A released chunk can't be taken back: re-anchoring does not reach chunks that wait in the
  decoder.
- Late video: sending old video to the queue is acceptable while there is nothing newer, and how
  late that frame is does not matter (up to the stale frame timeout). A backlog of late video is
  what has to be avoided.
- Late audio: it can be released or dropped, whichever is easier.
- A slew can go at most at 4% (40ms per second), so re-anchoring by 1s can take up to 25s.
- A jump is immediate but causes artifacts (an audible gap or skip, a video freeze or skip).
- Gaps in video can be treated more granularly than per input type (keep showing the old frame
  or not, from case to case). That changes the arguments passed to the queue, not the queue.
- The decoder has to receive a keyframe first, at the beginning and after every gap. That can be
  ensured in live sync or in a separate element after the live sync output; both are fine.
  - The gap until that keyframe needs no special handling in live sync: the output shows the old
    frame or falls back to black, depending on how gaps in video are treated in general.

### 2.2 All protocols

- **G1. Start.** The initial synchronization leaves room on both sides: packets delivered
  somewhat faster or slower than the first ones do not cause re-anchoring right after the start
  (G2). The buffer it lands on is the starting buffer `[today: desired]`. On HLS it lands there
  only roughly (H4).
  - The start can't use one of the limits `[today: min, max]`. At the upper limit a single
    faster packet already means squashing; at the lower limit a single slower packet already
    means stretching.
  - The starting buffer is just a value between the limits that is useful for the start. Picking
    it can be protocol dependent: half of the upper limit is fine in most cases; on HLS the
    segment size can force a larger value, so a bit under the upper limit is reasonable.
  - No claim is made about how the starting point is counted. The only expectation: whatever
    mechanism is used, it detects the initial burst and synchronizes the stream once delivery is
    more or less real time.
    - Every protocol can produce an initial burst (R2, H4, M12). Where the synchronization ends
      up does not depend on the size of the burst, within reason: 2s of media or 5s end up more
      or less the same.
    - Preferably it synchronizes first and produces packets only after delivery has stabilized.
      Otherwise the burst can exceed the limits and require stretching or squashing right at
      the start.
    - A burst of a few minutes of media is not within reason. The wait for the end of a burst
      has a timeout, and after it playback starts on a fallback. Which fallback is open.
      - `[today]` The wait is at most 2 x `desired` on RTMP and MoQ and 10s on HLS, counted from
        the first packet of the track. After it playback starts with whatever was measured.
    - HLS differs only in how long it takes to resolve the start properly (H4). The same logic
      may be able to handle both.
    - This is about the initial burst. A burst in the middle of the stream, e.g. after a gap,
      can be handled differently, depending on the case (G7, R3, H2).
  - Time to the first frame has no exact target and no hard limit. As a guide the maximum is 5s;
    with low latency configured preferably 1s, 2s is accepted. This is for normal conditions:
    with something weird on the network it can take longer.
    - Downloading the initial burst does not count: it has to happen. What is not wanted is
      waiting on top of it while nothing arrives. The start waits for the burst to end, so for
      the gap after it, and no longer.
    - Apart from that download, time to the first frame does not grow with the segment size.
      With 20s HLS segments, playback can't wait for one or two more segments to arrive: that
      would be 20-40s before the first frame (H1).
    - A starting buffer over 5s is fine on HLS, because 3 segments are downloaded ahead of time.
      Worst case with 2s segments: about 4s of black screen in the meantime. 
    - A large starting buffer can take the first frame past 5s, and that is fine because the
      user configured a buffer that large: RTMP with 10s shows the first frame about 9s after
      connect.
- **G2. Normal flow.** While the buffer does not go over the limits `[today: min, max]`, the
  playback timeline is not shifted: no stretch or squash, no dropped or repeated frames.
  - The limits come from the user: the user either defines them directly, or has options that
    select them indirectly and only partially.
  - How the buffer is compared with the limits is open: the redesign is not required to rely on
    the live edge estimate.
    - `[today]` The limits are resolved from the live edge estimate: the lower limit against the
      pessimistic one (the slowest packets), the starting buffer and the upper limit against the
      optimistic one (the fastest packets).
- **G3. A/V sync.** Tracks stay in sync, except in the cases listed in section 1.
- **G4. Slew or jump.**
  - Delivery suddenly gets later: a jump would be the best if it were known to be a new timeline,
    but the first late packets can't tell a burst that smooths out back to the old timeline from
    the start of a new one. Without that knowledge, slew.
    - This holds while playback has not run out of content (G13). Once it has, the artifact is
      already visible: it is a jump, or the content after the gap is treated as a fresh start
      (G1). Which one depends on the solution (G7).
    - A one-off burst must not leave latency behind for long. The buffer can start growing
      while the burst puts it under the lower limit `[today: min]`, but soon after the burst is
      done it stops growing, or it shrinks back to the size it had before, also when that size
      is inside the limits (an exception to G2).
      - The case to avoid: after a 5s gap the buffer slews until it is 5s larger and starts to
        shrink only minutes later.
        - `[today]` It happens: the burst counts as slow delivery for as long as the look-back
          window remembers it (problems, memory of the edge estimate).
      - A burst that is the normal delivery pattern is not one-off: an HLS segment (G7, H2).
      - A problem that comes back periodically is not one-off either: the buffer keeps covering
        it (M3).
    - A new timeline can start after a gap or without one, so it has to be possible to tell it
      apart from a burst of old content. How: section 3.
  - The buffer is larger than needed (getting back to the old timeline): a slew is fine.
  - In both directions a jump is acceptable when the shift is too large. The shift can be a false
    positive that newer data does not confirm.
  - The threshold for a jump depends on the solution (section 5), under one constraint: something
    common must not leave the stream needing a few minutes to get back to the optimal latency. If
    abnormal states can be detected and trigger a reset on their own, the threshold does not have
    to be low; otherwise hard resets have to be more aggressive. M1 lists states that have to be
    detected.
- **G5. Clock drift.** A source clock that runs too fast or too slow, by anything from a few ppm
  (100 ppm is 360ms per hour) up to the 4% a slew can absorb, runs for hours and days without a
  jump: nothing shifts while the buffer is inside the limits `[today: min, max]` (G2), and every
  time it reaches one it is slewed back (G4). More than 4%: G8.
- **G6. pts discontinuity.** Either direction. From the outside it is the delivery delay
  changing at once by the size of the jump, so a jump that can't be told apart from a change in
  delivery is G4. One that is known to be a new timeline is a fresh start, within the G1 target.
  - The two tracks can jump at slightly different arrival times, so packets of the old timeline
    and of the new one can arrive interleaved between the tracks: one track is already on the
    new timeline while the other still delivers the old one. Inside a single track they are not
    interleaved. That can be handled as one event or as two separate ones, as long as the other
    expectations hold: A/V sync holds after it (the state in between: G14).
  - A timestamp wrap is not part of this: it is handled on the protocol level (section 1).
  - How it can be detected, and what can be mistaken for it: section 3.
  - Consequences if it is not recognized:
    - a forward jump: nothing is shown for the length of the jump, and after that playback has
      that much extra latency;
    - a backward jump: all content is late by the length of the jump.
- **G7. Stall.** Delivery stops for so long that playback runs out of content, and then resumes
  in one of three ways. A gap that the buffer covers is not a stall: G13. While it is stalled
  there is too little information to tell the three ways apart. What happens afterwards is not
  strict: if frames or audio were already dropped, keeping the old timeline makes sense, but it
  can also be treated as a newly starting connection.
  - On HLS content comes in segments, so a burst of one segment is the normal pattern (H2).
    Knowing that does not rule out a burst of several segments, which is a stall resuming, as
    below. On MoQ the meaning of a burst is not known, so resyncing is the best bet.
  - Resyncing does not mean forgetting everything. Forgetting leads to a dangerous loop: all state
    is cleared on a stall, playback syncs to the starting buffer `[today: desired]`, that is not
    enough for a full segment, and content runs out again, because the timing of the previous
    segment was lost.
  - (a) Content continues where it stopped: the next packet continues from the pts where
    delivery stopped, with nothing skipped and no backlog. From then on every packet arrives
    later (the timeline has shifted). Jump to the new timeline: the stall has already caused a
    visible artifact.
  - (b) Content jumps ahead by the length of the stall (the source kept running, data was
    skipped). The old timeline still fits, so keeping it needs no re-anchoring; a fresh start is
    acceptable too, as stated above.
  - (c) The missed content arrives as a burst. Ideally the packets that should already have been
    presented are dropped and no re-anchoring happens. It is acceptable if the burst is
    interpreted as a new timeline and the buffer starts increasing by slewing (e.g. toward 15s
    more at 3%), as long as it does not stay that way once the burst is done: back on the old
    timeline, or a fresh start.
- **G13 Gap covered by the buffer.** Delivery stops and resumes before playback runs out of
  content, in one of the same three ways. Nothing is late (dropped) in any of them.
  - (a) Content continues where it stopped: the buffer is smaller by the length of the gap.
    Inside the limits `[today: min, max]` nothing happens (G2); under the lower limit it is
    slewed back (G4).
  - (b) Content jumps ahead by the length of the gap: the buffer is unchanged and the output has
    a hole of that length. Nothing is re-anchored.
  - (c) The missed content arrives as a burst: all of it arrives before it is due, so nothing is
    lost. The burst can put the buffer under the lower limit `[today: min]`: the one-off burst
    of G4.
- **G8. Source not real time.** Faster or slower than real time by more than a slew can absorb.
  - A file pushed as fast as possible: the behavior does not matter much. It still must not
    grow memory without bound.
  - Slightly above what a slew absorbs, e.g. 10%:
    - Faster: slew, and at some point resync. No opinion on the threshold.
    - Slower: the most optimal would be a reset to the starting buffer `[today: desired]` every
      time playback runs out of content. That might not work well with other cases where content
      runs out, so a later trigger for the reset, coming from different rules, is fine.
- **G9. Track added or removed** while the other track plays (late RTMP config, MoQ catalog
  change).
  - The playing track: the change by itself does not shift its playback timeline. It shifts only
    when the buffer no longer fits the limits `[today: min, max]` afterwards (G4, G15).
  - A track is added: it joins in sync with the playing track (G3).
  - A track is removed: what was measured on it no longer counts, and the remaining track is
    synchronized as a single-track input (G10). Nice to have, not a requirement.
    - `[today]` `(?)` Already the case: the estimator of the removed track is dropped with it,
      and the combined estimate comes from the remaining track alone.
- **G10. Single track.** Audio-only and video-only inputs behave as described in this section,
  and nothing waits for a track the input does not have.
  - An input where one track delivers and the other has stalled for good ends up with the same
    outcome as a single-track input. Right after the stall the two can differ; they have to be
    the same once some time has passed (1s, 10s or 1 minute, no strong preference).
- **G11. B-frames.** No special handling. With B-frames the pts of video are out of arrival order
  by the reorder depth (usually 2-3 frames), and that:
  - counts as jitter like any other, so such a stream needs that much more buffer (G2, G15);
  - can offset A/V by the reorder depth, which is accepted;
  - should be too small to be read as a discontinuity (section 3).
- **G12 End of stream.** Everything buffered is played out; the tail is not cut.
- **G14. Unknown sync point between the tracks** `[today: independent mode]`. A/V desync can be
  introduced here. This is not a strict requirement: it is unclear at what point it is certain
  that the timelines of the tracks diverged, and which cases count.
  - The tracks start from unrelated timestamps. The pts of audio and video for the same
    moment differ by an arbitrary amount, so the tracks can only be aligned by their newest
    content. Seen only on the MoQ input, possibly a bug of the sender. MoQ now has a concept of a
    clock: when it is set the tracks are known to be synchronized. It is optional, and without
    it nothing guarantees that.
  - Maybe also tracks that started together and diverged with time. Nice to have, not necessary:
    a broken Opus DTX implementation mishandles timestamps, so on every silence they fall
    further behind the correct values and the stream looks like its delay is growing very fast.
    With low latency configured such a stream should be handled reasonably: the chunks are just
    played as soon as they are received, by aggressive re-anchoring or by a different mechanism.
  - The intermediate state around a discontinuity on HLS (one track jumped, the other not yet) is
    a place where it could be triggered, but there can be solutions that do not treat that state
    as an unknown sync point.
  - Treating the sync point as unknown can be useful where it is not strictly necessary (M2):
    - the primary track, audio, is on time while the second one is lagging, and the user wants
      audio latency at the cost of desync;
    - on a bad network video lags more and more behind: it should not affect audio, but it is
      still nice to have at least one frame here and there.
- **G15. Delivery does not fit in the configured limits** `[today: min, max]`. On every protocol,
  when the limits are too small to handle the stream, the stream still has to work, but it does
  not have to be perfect.
  - Priority: from experience this is the requirement most likely to be sacrificed. Even so, it
    is important: the design has to make an effort to make it work before giving any of it up.
  - Examples: an HLS segment larger than the starting buffer or the upper limit
    `[today: desired, max]`; on any protocol, jitter larger than the distance between the limits
    `[today: the distance between the lower and upper bound larger than max - min]`, e.g. low
    latency configured with an upper limit of 400ms while the delivery delay varies by 700ms
    from packet to packet.
  - What "still works" means is configurable and different per protocol:
    - Inputs whose transport can't drop packets (RTMP; HLS technically can, but not in our
      setup): when the network throughput is not enough, playback resynchronizes with the
      preferred approach, jumping or slewing (G4, R1). That is not a promise that no data is
      lost: late content can still be dropped (R3). H2 is the segment case of this.
    - Inputs whose tracks have independent transport and can drop parts (MoQ): two behaviors,
      M2.
- **G16. One track pauses while the other continues**, with nothing wrong on the network (audio
  that is not sent while muted, a static screen share). Missing data is not a sign of delay: a
  pause does not cause re-anchoring, on the paused track or on the other one.
  - Nothing waits for the paused track. When it resumes with the same delivery delay it is in
    sync without re-anchoring.
  - At the output a pause in audio is just no audio, and a pause in video can mean that the
    last frame stays until the next one arrives (2.1, gaps in video).
    - `[today]` RTMP and HLS stop showing the frame after 3s; MoQ keeps it.
  - Whether live sync treats the paused track as stalled or as still running is up to the
    solution.
  - R4 is the short version of this.

### 2.3 HLS

Delivery pattern:

- FFmpeg demuxes on one thread and hands out audio and video packets from the same read loop, so a
  delay always hits both tracks together. Nothing is lost or reordered.
- Content arrives in whole segments: the packets of a segment come as a burst at download speed,
  then nothing until the next segment is published and picked up by the playlist reload. Segment
  N+1 arrives about one segment duration after N, plus CDN and reload jitter.
- FFmpeg starts 3 segments before the end of a live playlist (`live_start_index=-3`), so the first
  burst holds about 3 segments of content.
- Segment duration is usually 2-10s and can be 20s or more.
  - `[today]` Live sync is not told what it is; the user is expected to size `buffer` for it
    (defaults: desired 10s, min 2s, max 20s).
- Audio and video share a timeline. `EXT-X-DISCONTINUITY` and encoder restarts change the pts base;
  MPEG-TS pts wrap every 26.5h. Checked in the FFmpeg source: the discontinuity tag is only
  counted, so the new pts base arrives as a pts jump of any size, in either direction, without a
  flag (`hls.c`, `parse_playlist`). The first wrap after opening is unwrapped by libavformat and
  does not reach us (`demux.c`, `wrap_timestamp`).
- A playlist with `EXT-X-ENDLIST` is not live and takes the non-live path.

Scenarios:

- **H1. Long segments.** With 20s segments we can't wait for multiple segments before producing:
  not 20-40s before the first frame, the target is G1.
- **H2. Starting buffer smaller than a segment** `[today: desired]`. The buffer option assumes
  the user knows the upper limit of the segment size, but e.g. a starting buffer of 10s with 20s
  segments still has to work. Artifacts at the start are acceptable.
  - Seen from the outside: after the initial batch (3 segments) a new segment arrives about
    every 20s, as a burst. Playback is continuous only when the last packets of a segment have
    at least a whole segment of buffer, which is more than was configured (G15).
  - Expected: playback becomes continuous, with a buffer larger than configured. Before
    that it can run out of content, but it must not keep running out on every segment (G7).
  - If playback ran out of content, then when the next segment is received, one of:
    - playback is shifted so that the start of the new segment is now; nothing is dropped;
    - the late packets are dropped and the rest is slewed. Dropping does not make sense when it
      is already known that playback is behind, but it is acceptable if it fits the solution
      better;
    - playback is shifted so that the start of the new segment is on the starting buffer; no
      slewing is necessary. Reading: the first packets of the segment wait for as long as
      the starting buffer before they are presented.
  - A shift is reasonable here because the content was already interrupted.
  - `[today]` How the current implementation goes through it: problems, memory of the edge
    estimate.
  - HLS knows that content comes in segments, so a more optimal solution can be made than for an
    input where the meaning of a burst is unknown (G7).
  - With small segments a slew might not be enough, but that should not happen: e.g. the lower
    limit `[today: min]` should never be set lower than 2s. HLS with a lower limit under 2s is a
    situation that is not cared about (see the top of this section).
- **H3. No large late batches.** Content that is already behind the queue is dropped instead of
  being released to the decoder, which would take a while and show as very fast sped-up video.
  - This is about the amount of late video (2.1). A late frame with nothing newer can still be
    released, and late audio can be released or dropped.
- **H4. Initial batch.** When the initial batch (3 segments) ends, it is known how much content
  it holds, but not when the next segment will be published: that can be anything up to one
  segment duration away, and the start can't wait for it (H1). So the initial synchronization
  can't land on the starting buffer `[today: desired]` precisely, and on HLS that is not
  important.
  - The start can still aim at the starting buffer, e.g. to share the implementation with the
    other protocols.
  - A buffer that does not match the starting buffer is preferred to an artifact.
  - The size of a segment is not known at that point either: the batch is several segments at
    once, and live sync does not know how many. How slow the slowest packets are can be judged
    only on what arrives after the batch.
  - The starting buffer still counts later: once a limit `[today: min, max]` is hit, it is
    where the buffer is brought back to (G4).
  - A start that lands precisely, with no stretching or squashing afterwards and no extra
    delay, is welcome, but no good way to get it is known for now (section 4).
  - `(?)` The batch can hold more content than is played. As G1, with the part that is not
    played handled as H3.
  - `(?)` When the start cuts into the middle of a GOP, video starts at the next keyframe, and
    the gap until then gets no special handling (2.1). The same holds wherever playback starts
    or resumes on the other protocols (G1, R3, M3, M12).
    - `[today]` Video starts at the cut: the part of the GOP before it is decoded and not
      shown.
- **H5 `(?)` Segment arrival jitter.** A segment published or fetched late. While the buffer covers
  it nothing changes (G2). When it does not, playback runs out of content: as H2 from there on.
  Latency added that way is removed only when it is over the limits `[today: min, max]` (G4).
- **H6 `(?)` Segment duration changes mid-stream** (origin reconfigured, ad break with different
  segments). Longer segments that no longer fit in the buffer: as H2. Shorter ones:
  nothing shifts unless the buffer is over the limits `[today: min, max]` (G2, G4).
- **H7 `(?)` Origin or CDN stall.** No new segment for longer than the buffer, then several
  segments at once, or FFmpeg skips the expired ones, or the source was really paused. As G7.
- **H8 `(?)` Discontinuity** (ad insertion, encoder restart), including pts jumps under the
  detection threshold `[today: 10s]`. As G6.
- **H9 `(?)` Download slower than real time for a long time.** As the slower case of G8; nothing
  can be dropped in our setup (G15, R1).
- **H10 `(?)` Short segments with a small buffer** (1-2s segments, a starting buffer around 3s,
  a lower limit not under 2s as in H2) `[today: desired, min]` work as a lower-latency setup.
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
- `[today]` A new connection gets a new live sync instance and a new queue track group, after
  the previous connection ended.
- `ingest_mode: offline` takes the non-live path.
- `[today]` Defaults: desired 2s, min 1s, max 5s.

Scenarios:

- **R1. Shared connection.** Everything goes over a single TCP connection, so the transport can't
  drop a part of the stream. If the network throughput is not enough, playback resynchronizes
  with the preferred approach, jumping or slewing (G4, G15). This does not mean that no data can
  be lost: late content can still be dropped (R3). Giving up on a late track while the other is
  on time (M2) is not acceptable here.
- **R2 Startup burst.** As G1: whatever the size of the burst, it does not turn into latency.
- **R3. Gap, then a burst of backlog.**
  - A 5s gap, then the missing data arrives as a backlog and the stream stays on the old
    timeline: the late frames are dropped (G7(c), H3).
  - A longer gap, or the start of one of the tracks, followed by a burst: playback is synchronized
    to the starting buffer `[today: desired]`, as at the start (G1). A burst larger than that
    loses its older part; with a smaller one playback does not start immediately.
  - `(?)` Reading: where "longer" begins is decided by the solution (section 5). A track that
    starts while the other one plays joins the playback timeline of the playing track (G3, G9),
    which is the same thing as long as that track sits at the starting buffer.
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
- The protocol layer knows group sequence and frame index (order, gaps, keyframes).
  - `[today]` It reads all open groups concurrently and drops groups that can't be played
    anymore.
- Timestamps are pts in µs per frame, without dts.
- The catalog can carry an optional `clock`, the one continuous clock of the broadcast. When it is
  set the tracks are on one timeline; without it that is not guaranteed (G14).
  - `[today]` Our MoQ output sets it; the MoQ input does not read it.
- `[today]` A publisher restart or relay reconnect is a new broadcast: new live sync instance and
  new queue track group. A catalog change restarts the track of that kind inside the same
  instance.
- Network conditions change during a session more than for the other protocols, and the target is
  low latency.
  - `[today]` Defaults: desired 2s, min 1s, max 5s, audio prioritized.

Scenarios:

- **M1. Recovery from a severely degraded network.** Examples: throughput lower than the bitrate,
  so audio is more or less on time while video is either missing or 10-20s behind; 90-100% packet
  loss for a short or a long time. This is not exotic: it happens when driving through an area
  with bad or no coverage. Such cases have to be detected and followed by a quick clean start, so
  that within a few seconds the stream is back at a good latency with correct A/V sync. A clean
  start is a resync, which does not mean forgetting everything (G7).
  - "Recovered" is not exact: with a 200-500ms latency target, ending up at 1s is noticeable, but
    even 1-2s counts as recovered. What is left is slewed (G4).
- **M2. One track can't keep up.** One track can't be on time together with the other one.
  Examples: with low throughput video can't be delivered in real time while audio is on time; or
  the maximum buffer is a hard 500ms (which assumes an option like that is exposed) while audio
  is received 1s earlier than video, so video is never on time. Both tracks have independent
  transport and can drop parts, so two behaviors might be useful:
  - Reference track (expected only on low-latency MoQ): one track is picked as the reference. The
    other track can affect it, but the impact has to be limited: there must be some limit, and
    whether the upper limit `[today: max]` is the boundary is up for discussion. A solution where
    the second track has no impact at all is not ruled out, although problems are suspected with
    it. Late frames of the other track can be dropped or shown late; shown late seems better,
    otherwise a consistently delayed track shows just one image.
    - `[today]` Hardcoded for MoQ: the reference is audio if it exists, and video can grow the
      buffer only as far as shifting audio to `max`, no further.
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
  divergence was large (M2).
  - How fast it is depends less on how the playback timeline is moved (slew or jump, G4) than
    on how soon it is noticed that the conditions changed.
  - A problem that comes back periodically is not a change of conditions: the buffer keeps
    covering it and does not shrink in every good period. Telling the two apart: section 3.
  - `(?)` It rejoins in sync within a few seconds, without visible fast-forward. Where inside a
    group it resumes: H4.
- **M4 `(?)` Network gets worse** (delivery delay steps up, more jitter). As G4 while delivery still
  fits in the limits `[today: min, max]`; when it does not, G15 and M2.
- **M5 `(?)` Isolated spike** (one late group, a 1s hiccup). If it pushed the buffer over the
  limits `[today: min, max]`, playback is back inside them within what G4 allows: seconds to tens
  of seconds, not minutes. Latency added inside the limits can stay or be taken back (the
  one-off burst of G4).
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
- **M12. Initial burst.** As G1, the same as on the other protocols. On MoQ the burst is the
  groups the relay still holds within the max age of our subscription `[today: set to max]`,
  each from its start, so it can hold about that much content and not only the current GOP.

### 2.6 Open questions

Conflicts between requirements, with specifics. Everything else that is not confirmed is a `(?)`
in its scenario.

None at the moment.

## Problems with the current solution

What today's live sync does not handle well, optimally or as desired. The references in
parentheses point to the scenario that describes the same situation. How the implementation
works: appendix.

Memory of the edge estimate:

- **The edge estimate has to remember and to forget at the same time.**
  - It keeps a long memory. Without a reset, when network latency improves it takes a long time
    to notice that the lower edge changed. The same happens when one packet, or a few, are very
    late and delivery is back to normal afterwards: the buffer usually keeps slewing up toward
    the value those few packets define, and only after they are forgotten it starts going down
    (M1, M3, M5).
    - Another case: a backlog flushed after a gap counts as slow delivery until it leaves the
      look-back window of 20-120s. After a 5s gap the buffer slews toward 5s more, which takes
      a while, and it can start to shrink only once the backlog has left the window: a few
      minutes of unnecessary latency (G4, G13(c)).
  - Resetting it whenever something weird happens, or when playback runs out of content, forgets
    cyclic spikes. Example: with the starting buffer `[today: desired]` smaller than an HLS
    segment, playback runs out of content on every segment (H2, G7). The flow:
    1. The initial batch (3 segments) is downloaded.
    2. Everything except the last part of it, as long as the starting buffer, is dropped.
    3. Playback starts.
    4. It runs out of content before the next segment is received.
    5. `(?)` 1s later the tracks start over and everything measured so far is forgotten. The
       next segment is handled as a new start, so it goes on from step 2.

Late content:

- `(?)` On RTMP a late backlog is not held back from the decoder: all of it is released, marked
  as not to be shown, and costs decoder time. On MoQ that goes only for what was read before
  the input gave up on the late groups. The HLS input holds it back, unless a packet of the
  same GOP is on time (H3, R3, G7(c)).

Re-anchoring:

- A source that drifts faster than the slew rate (3% when shrinking the buffer, 4% when growing
  it) is not kept up with (G8).
- `(?)` There is no jump. Anything a slew can't cover waits for a track to start over (delivery
  stopped and content ran out, or the estimate is stale) (G4).

pts discontinuities:

- A discontinuity is detected only as a pts jump of over 10s between consecutive arrivals. A
  smaller jump is taken at face value. Forward: the output freezes for the length of the jump,
  and the extra buffer is slewed away only if it is over the upper limit. Backward: `(?)`
  content can be late until the buffer is slewed up or, after a larger jump, until the stale
  estimate makes the track start over (G6, H8, R5).
- `(?)` On MoQ the same detection can fire on a late group when `max` plus the length of two
  groups is over 10s, because groups arrive out of order and the reader keeps groups up to
  `max` old (M7).

A/V sync:

- `(?)` Whether the tracks share a timeline is detected only from how far apart their live edges
  are, the optimistic ones or the pessimistic ones: over 5s means unrelated timelines, under 3s
  a shared one. The MoQ input does not read the `clock` of the catalog (G14).
- Between those two thresholds the offset between the tracks is not re-aligned. If the leading
  track was slewing when the re-alignment stopped, the offset keeps what was left of that slew
  (G3).

Limits:

- `(?)` Low latency can't be forced. When delivery varies by more than `max` - `min`, keeping
  `min` for the slowest packets wins and the buffer goes over `max`. That also holds for the
  reference track on MoQ (section 1, G15).
- `(?)` The live sync buffer has no size limit (G8).

## 3. Signals: what can be auto-detected and at what cost

TBD. For every unreliable detection it has to list the failure cases, the state each one leads
into, whether that state is recoverable and how quickly (section 1).

Detections mentioned so far (examples, not decisions):

- **pts discontinuity (G6).** The most likely way to detect it is a numerical threshold, large
  enough that a gap caused by network issues is unlikely to be mistaken for it.
  - A different threshold can be used when the new timestamps overlap with the current ones: an
    overlap can't be mistaken for a gap.
  - The threshold can be the same for every protocol, but does not have to be.
  - It has to be a lot larger than the backward steps B-frames cause (G11).
  - `[today]` 10s on every protocol. Forward, the jump is measured against the pts expected from
    the time since the last arrival; backward, against the last pts.
  - Where it can be misdetected:
    - MoQ with network throughput below the bitrate: audio arrives on time while video stays
      more and more behind, so the live edges of video and audio diverge further and further
      (M2).
    - One track stops being sent and starts again after 30s: consecutive timestamps differ by
      30s, but it is not a discontinuity (G16). Depending on the solution it might not be
      necessary to detect a discontinuity on a track that ran out of content, and then this
      example is not relevant.
- **A burst of old content or a new timeline (G4).** Both begin the same way: packets arrive
  later than before, after a gap or without one. A burst ends back on the old timeline and must
  not leave latency behind; a new timeline stays and has to be followed. A way to tell them
  apart is needed; which one is TBD.
- **Changed conditions or a periodic problem (M1, M3).** Delivery got better. Either the
  conditions changed, and how soon that is noticed decides how fast latency recovers; or it is
  the good period of a problem that comes back, which the buffer has to keep covering. The two
  pull in opposite directions: noticing a change fast means forgetting, covering a periodic
  problem means remembering.
  - `[today]` One look-back window serves both, with resets on top: problems, memory of the
    edge estimate.
  - `(?)` A problem seen once is one-off (G4, M5); it becomes periodic when it repeats. How long
    a repeat is waited for, so how long a period can still be covered, is TBD.

## 4. Hints passed by the input

TBD.

Hints mentioned so far (examples, not decisions):

- Content is delivered in segments (HLS). With it, a burst after a pause is the normal pattern;
  without it (MoQ) the meaning of a burst is unknown. It does not rule out a burst of several
  segments (G7).
- `(?)` The tracks share one timeline: always on HLS and RTMP, on MoQ only when the catalog sets
  the optional `clock` (G14). For MoQ that is information extracted from a specific stream, so
  it falls under the case-by-case rule of section 1.
- `(?)` How long ago the newest segment was published (HLS). At the end of the initial batch
  that is the one thing missing to land on the starting buffer precisely (H4); from timing
  alone it is known only when the next segment arrives.
  - Not for now. The playlist can carry it (`EXT-X-PROGRAM-DATE-TIME`, or the HTTP headers of
    its response), but parsing the playlist independently from FFmpeg is not wanted. It stays
    possible only if FFmpeg exposes it, which was not checked.

## 5. Solution

TBD. It has to list every requirement that is abandoned, each with its reasoning (section 1).

`(?)` A full proposal for sections 3-5 is in `DESIGN.md`, next to this file.

Ideas mentioned so far (examples, not decisions):

- Inputs that can't drop packets: on every stall reset to the starting buffer
  `[today: desired]`; when just below the lower limit `[today: min]` without stalling, slew.
- An extreme low-latency mode: everything is played as soon as it is received and nothing is
  synchronized, so no buffer is kept and the tracks are not aligned with each other. Something
  to consider, not a chosen direction.
  - If it is offered to users, it has to be an explicit option (section 1, small public API).
  - It would be one more case where A/V desync is introduced (section 1), and one way to get
    what G14 describes for the broken Opus DTX stream ("played as soon as they are received").

## Appendix: current implementation

Reference only, as of `62d11b67`. Found by reading the code, not by running it. What is not
handled well: "Problems with the current solution".

### Approach

The playback timeline is set once, so that the packets delivered fastest have `desired` of
buffer. After that it moves only when the buffer is measured outside `min`..`max`, and then only
by slewing. What a slew can't fix is handled by making a track start over.

The rules it follows:

- **Everything is judged from `arrival - pts`.** Each track remembers its fastest and its
  slowest delivery over a sliding window. The fastest one gives the optimistic live edge (the
  pts that would be arriving right now), the slowest one the pessimistic edge.
  - The window is 20-120s long: 4 times the largest gap between arrivals of the last 60s, so
    that it holds several HLS segments.
  - The fastest delivery is stable when no chunk has beaten it by more than
    `stabilization_tolerance` for `stabilization_period`. Only chunks that arrive while it is
    stable count for the slowest delivery, so a backlog flushed at the start is not read as
    slow delivery.
- **A track starts once the fastest delivery is stable.** Until then its chunks are held. It
  waits for every track that delivers, but for at most `max_wait` from its own first chunk.
  - The first track to start sets the playback timeline: the fastest delivery gets `desired`
    of buffer, or more if the slowest one would get less than `min`.
  - A track that starts later joins the playback timeline of the playing track.
- **Inside the limits nothing moves.** The buffer is inside them when the slowest delivery has
  at least `min` and the fastest at most `max`. When both can't hold, `min` wins.
- **Outside the limits the playback timeline is slewed**, toward where a start would put it now.
  It does not stop at the limit: it goes on until the fastest delivery is back at `desired`
  (or the slowest at `min`). There is no jump.
  - It advances with the released content, by a share of its duration: 1-4% when growing the
    buffer, 1-3% when shrinking it. The top rate needs at least `desired` left to go when
    growing and 3 x `desired` when shrinking; the end of every slew runs at 1%.
  - Counted from that: 1s takes about 60s when growing and about 85s when shrinking with the
    RTMP and MoQ defaults; 10s with the HLS defaults takes about 8 and 13 minutes. A source
    that runs 2% too fast is kept up with only once the buffer is at 2.5 x `desired`.
- **What a slew can't fix makes the track start over.** Its buffered chunks are released, what
  was measured on it is forgotten, and it waits for a start again. That happens when:
  - the pts jumps by over 10s between two consecutive arrivals of the track (a discontinuity);
  - delivery stopped and the released content ran out, both at least 1s ago;
  - the estimate is stale: the fastest of the most recent deliveries (the current 2s bucket, or
    2 arrival gaps back when that is longer) is behind the fastest of the whole window by
    `stale_estimate_threshold`;
  - the stream ends or the track is removed.
  - A track that starts over alone rejoins the track that kept playing. When both start over,
    the playback timeline is set from scratch, and that is the only jump of the playback
    timeline.
- **Two tracks share one playback timeline** while their timelines look related.
  - The limits are then judged on both tracks together (the fastest delivery of either, the
    slowest of both), and chunks are released in pts order across the tracks, so a slew moves
    both the same way.
  - With a priority track (MoQ: audio) the other track can grow the buffer only until the
    fastest delivery of the priority track has `max`. The slowest delivery of the priority
    track still gets `min`.
  - The timelines count as unrelated when the live edges of the tracks, or their slowest
    deliveries, are over 5s apart, and after a discontinuity on one track. Audio then leads on
    what is measured on it alone (video while there is no audio), and the other track is placed
    so that both live edges are presented together. The offset between the tracks slews at
    25%.
  - Under 3s apart the tracks share the playback timeline again: the offset is slewed to zero,
    and the last 200ms are dropped at once. In between nothing changes.
- **Chunks are released shortly before they are due:** not more than 5s ahead, so that a slew
  still reaches what is buffered. A chunk behind a gap that might still be filled (jitter
  buffer), or one that waits for the other track to deliver, is released 100ms ahead at the
  latest.
  - A late chunk is marked decode-only (decoded, not shown), except the newest video frame in
    the buffer, which is always shown.
  - The input is told the oldest pts that can still be presented.

### Per protocol

| | RTMP | HLS | MoQ |
| --- | --- | --- | --- |
| Buffer default (min / desired / max) | 1s / 2s / 5s | 2s / 10s / 20s | 1s / 2s / 5s |
| `track_priority` | `None` | `None` | `Audio` |
| `stabilization_period` / `_tolerance` | 500ms / 100ms | 1s / 250ms | 500ms / 100ms |
| `max_wait` | 2 x desired | 10s | 2 x desired |
| `stale_estimate_threshold` | 2s | 10s | 500ms |
| Buffer type | FIFO | FIFO | jitter buffer by (group, index) |

- Option floors (min / desired / max): HLS 500ms / 1s / 1.5s, RTMP 200ms / 400ms / 600ms, MoQ
  100ms / 200ms / 300ms.
- Late content: RTMP sends all of it to the decoder. HLS holds it back unless a packet of the
  same GOP is on time. MoQ sends what it has read, but stops reading groups that can't be
  presented anymore.
- A discontinuity is passed on to the decoder only by the HLS input; the RTMP and MoQ inputs
  ignore it.
- Only the HLS input makes sure the decoder gets a keyframe first. The RTMP and MoQ inputs do
  not, not even after a chunk was dropped because the decoder input was full.
- A timestamp wrap is not handled by any input. RTMP passes the 32-bit timestamps on as they
  are, and on HLS every wrap after the first reaches live sync (2.3). Both arrive as a pts
  jump over 10s, so as a discontinuity.
- The live sync buffer has no size limit.

### Where in the code

- Overview and known issues: `mod.rs`.
- Live edge estimate (min/max of `arrival - pts` over an adaptive window): `edge_estimator.rs`.
- Limits, where a slew is headed, slew rates, track priority: `buffer.rs`.
- Start and re-anchoring decisions, shared and unrelated timelines: `decision_start_track.rs`,
  `decision_correct_mode.rs`, `mode.rs`; applied in `state.rs`.
- Protocol layers: `pipeline/hls/hls_input.rs`, `pipeline/rtmp/rtmp_input/connection.rs`,
  `pipeline/moq/input/{connection,track_reader,buffer}.rs`.
- Downstream: `queue/{video_track,audio_track}.rs`, `audio_mixer/input/resampler.rs`,
  `pipeline/decoder/decode_only_filter.rs`.
