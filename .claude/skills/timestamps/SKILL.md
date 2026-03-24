---
name: timestamps
description: Load documentation on how PTS timestamps are handled across smelter — input sources (MP4 vs RTP/WHIP/WHEP), queue input/output, audio mixer drift correction, and API offset mapping. Use when working on or asking about timestamp-related code.
disable-model-invocation: false
allowed-tools: Read
---

Use the following context about timestamp handling for the current task.

# Timestamp Handling

All internal PTS (presentation timestamp) values are `std::time::Duration` measured from a single `sync_point: Instant` created at pipeline startup. The authoritative reference is the queue comment in `smelter-core/src/queue.rs`.

## Input-side timestamps

**MP4 inputs** (`smelter-core/src/pipeline/mp4/mp4_input.rs`) — Timestamps come from the MP4 container (correct relative to stream start). The input thread reads `offset = sync_point.elapsed()` when it starts, then adds that to every chunk PTS/DTS: `chunk.pts += offset`. After that, an `InputBuffer` duration is added (`chunk.pts += buffer.size()`). This makes the PTS relative to `sync_point` and accounts for buffering latency. The buffer is not applied for required inputs or inputs with a user-defined offset.

**RTP / WHIP / WHEP inputs** (`smelter-core/src/pipeline/rtp/rtp_input/rtcp_sync.rs`) — RTP timestamps are raw 32-bit counters at a codec-specific clock rate (90 kHz for video, 48 kHz for Opus, etc.). `RtpTimestampSync` converts them:
1. On the first packet, records `sync_offset = sync_point.elapsed()` (best-effort alignment).
2. Subtracts the first RTP timestamp to zero-base the stream.
3. Divides by clock rate to get seconds, adds `sync_offset`: `pts = (rtp_ts - rtp_offset) / clock_rate + sync_offset`.
4. Optionally refines via RTCP Sender Reports + NTP for cross-track sync.
5. Handles 32-bit RTP timestamp rollover.
6. Packets then pass through a jitter buffer before reaching the decoder.

Key difference: MP4 timestamps are container-relative and offset once; RTP timestamps require clock-rate conversion, rollover handling, and optional NTP refinement.

## Queue timestamps

The queue consumes decoded frames and produces synchronized batches for rendering/mixing.

**Input side** — When a frame enters the queue (`smelter-core/src/queue/video_queue.rs:try_enqueue_frame`):
- **No user offset**: frame PTS is used as-is (already relative to `sync_point` from the input stage). `first_pts` is recorded for the stream.
- **With user offset**: PTS is rebased to `frame.pts = (offset_pts + frame.pts + pause_offset) - first_pts`, where `offset_pts = queue_start_pts + user_offset`.

**Output side** — The queue thread generates output PTS deterministically:
- Video: `buffer_pts = sent_batches_counter * frame_interval + queue_start_pts` (one frame per output tick).
- Audio: 20ms chunks, `(queue_start_pts + 20ms*i, queue_start_pts + 20ms*(i+1))`.
- For non-required frames, a real-time deadline `sync_point + pts` is enforced — late frames are dropped.

**Public vs internal PTS**: `queue_start_pts = sync_point.elapsed()` at queue start. All internal PTS values are relative to `sync_point`. Public PTS (used outside the queue) is `internal_pts - queue_start_pts` (i.e., time since queue start).

## Audio mixer timestamps

The audio mixer (`smelter-core/src/audio_mixer/`) receives `InputSamplesSet` with `(start_pts, end_pts)` ranges and produces continuous output.

- **Per-input resampling**: Each input has an `InputResampler` that converts from the input sample rate to the mixing sample rate. The resampler tracks timestamp drift between expected and actual PTS.
- **Drift correction**: If input PTS drifts from expected PTS by more than 2ms (`SHIFT_THRESHOLD`), the resampler adjusts its ratio to stretch or squash audio (up to ±5%). If drift exceeds 500ms (`STRETCH_THRESHOLD`), samples are dropped or zero-filled instead.
- **Gap handling**: Gaps > 80ms (`CONTINUITY_THRESHOLD`) in input are filled with silence. The mixer guarantees continuous output — gaps are always zero-filled.
- **Output PTS**: The mixer's output `start_pts` matches the requested `start_pts` from the queue. Output is always continuous with no gaps.

## API offset vs internal timestamps

The HTTP API exposes `offset_ms: Option<f64>` on all input types (MP4, RTP, RTMP, WHIP, WHEP, HLS). This is converted to `Duration` in `smelter-api/src/input/queue_options.rs` via `Duration::from_secs_f64(offset_ms / 1000.0)` and stored as `QueueInputOptions.offset`. The offset represents the delay from queue start before the input begins playing. Internally, `offset_pts = queue_start_pts + offset` is the absolute PTS threshold — frames for that input are not emitted until `buffer_pts > offset_pts`.
