# Handoff: Why MoqStreamer audio+video plays as audio-only on smelter

## Focus

This document explains **the reason** the bug happens, so it can be fixed in **smelter**
(`server/`). The browser tool is publishing valid, spec-consistent streams — the defect is in
how smelter normalizes timestamps across tracks.

## Symptom

Publishing **both** audio and video from `tools/src/tools/MoqStreamer.tsx` (via
`tools/src/moq/publisher.ts`) to the smelter MoQ server renders **only audio**. Video is
silently dropped. Independent of container (CMAF / Legacy / LOC) and codec. Does **not**
reproduce with the Rust `moq-cli` publisher against the same server.

## The reason (root cause)

Two facts collide:

1. **Smelter normalizes all tracks against a single shared "first PTS".**
   `server/smelter-core/src/pipeline/moq/input/connection.rs`
   - `first_pts` is one `Arc<Mutex<Option<Duration>>>` created at `connection.rs:132` and
     **shared by both** the video and audio track tasks.
   - `normalize_pts()` (`connection.rs:475`) does `raw_pts.saturating_sub(first)`, where
     `first` is set by whichever track delivers its first frame. That track defines the zero
     point of the timeline **for both tracks**.

2. **A browser publisher legitimately puts audio and video on different timestamp epochs.**
   Chrome's `MediaStreamTrackProcessor` gives `AudioData.timestamp` an effectively ~0-based
   value while `VideoFrame.timestamp` is a large capture-clock value. The publisher writes
   these WebCodecs timestamps through unmodified (`tools/src/moq/publisher.ts`
   `writeVideoChunk`/`writeAudioChunk`, ~`publisher.ts:559-624`). This is valid MoQ — each
   track carries a self-consistent, monotonically increasing timeline.

**Sequence that produces the bug:** audio begins encoding immediately, but video waits for
its first keyframe + encoder config, so **audio's first frame reaches smelter first** and
sets the shared `first_pts` to audio's small base. Video's large timestamps minus that small
base stay large, so every video frame lands far in the future on smelter's timeline. The
queue buffers up to `MOQ_MAX_BUFFER` (20s) and then drops them → **only audio is presented**.

## Why the other cases match this explanation

- **Video-only works** — video sets `first_pts` itself, so its frames normalize to ~0.
- **`moq-cli` works** — it publishes an fMP4 file whose audio and video already share one
  timescale/epoch, so the shared `first_pts` happens to be correct for both.
- **Container/codec-independent** — the defect is in the shared timestamp base, not the
  packaging.

## Fix direction (in smelter)

Do **not** change the tool. The tracks arrive on independent, self-consistent epochs, so
smelter must not force them to share one `first_pts`. Investigate normalizing **per track**:

- Give each track task its own first-PTS reference instead of the shared
  `Arc<Mutex<Option<Duration>>>` at `connection.rs:132`, so audio and video each normalize to
  their own ~0 start.
- Verify this against the existing A/V-sync mechanism (shared `TrackOffset`, queue inputs in
  `server/smelter-core/src/queue/`) — the reason `first_pts` was shared may have been to
  preserve relative sync between tracks. Confirm per-track normalization still keeps audio and
  video aligned (each stream's first chunk corresponds to ~the same wall-clock capture start,
  so independent rebasing should preserve sync), or introduce a sync-preserving alternative.

## State of the work

- Diagnosis complete. A plan file exists but it targets a tool-side workaround
  (`/home/jbrs/.claude/plans/i-have-a-problem-quizzical-quasar.md`) — treat it as **superseded**
  now that the fix moves into smelter.
- No code changes made. `git status` was clean at session start.

## Suggested skills for the next session

- `/codebase-analyzer` — to map the smelter MoQ input timestamp/sync path
  (`connection.rs` `normalize_pts`, the shared `first_pts`, and the queue `TrackOffset`) before
  changing normalization.
- `/verify` — after implementing the smelter change, to confirm per-track normalization fixes
  the drop without breaking A/V sync or the video-only path.
