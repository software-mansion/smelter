# Handoff: Fix cross-epoch A/V drop in smelter MoQ input using the live-edge method

## Focus

Replace smelter's single shared `first_pts` normalization with a **per-track live-edge
aligner** ported from `moq-kit`'s playback pipeline. This fixes browser publishers whose
audio and video ride on independent timestamp epochs (video is silently dropped today)
**without breaking A/V sync** for native / `moq-cli` publishers that already share one epoch.

Target file: `server/smelter-core/src/pipeline/moq/connection.rs`.

## Root cause (recap, verified against current code)

- `first_pts: Arc<Mutex<Option<Duration>>>` is created once (`connection.rs:174`) and shared
  by both track tasks via `TrackCtx` (`connection.rs:60`).
- `normalize_pts()` (`connection.rs:516`) does `raw_pts.saturating_sub(first)`, where `first`
  is set by **whichever track delivers its first frame** (`get_or_insert`).
- Browser (`MediaStreamTrackProcessor`) gives `AudioData.timestamp` a ~0-based value and
  `VideoFrame.timestamp` a large capture-clock value. Audio encodes first, so audio's first
  frame sets the shared base to a small number. Video's large timestamps minus that small
  base stay large → every video frame lands ~capture-clock-seconds in the future → buffered
  up to `MOQ_MAX_BUFFER` (20s, `connection.rs:38`) then dropped → **audio-only output**.

The two tracks are each self-consistent and monotonic; the defect is forcing them onto one
zero point that only one of them defines.

## The method: how moq-kit solves the identical problem

moq-kit does **not** rebase each track to its own zero (that fixes the drop but silently
breaks lip-sync, because two independent zero points throw away the real inter-track
relationship). Instead it measures the offset between the two timestamp domains **at runtime
from frame arrival wall-clock**, and remaps video into the audio domain. Reference source:

- `ios/Sources/MoQKit/Subscribe/internal/playback/MediaLiveEdge.swift` (and `.../android/.../MediaLiveEdge.kt`)
  — one instance **per track**. Records a running `maxOffset = max(pts − wallclock_now)`;
  estimates the track's current live PTS as `wallclock_now + maxOffset`.
- `ios/.../playback/MediaTimestampAligner.swift` (`.../android/.../MediaTimestampAligner.kt`)
  — holds one live-edge per kind, derives `offset = audioEdge − videoEdge`, applies it only
  when it exceeds a threshold, and maps video timestamps into the audio domain.
- `ios/.../playback/VideoRenderer.swift` — threshold `ptsCorrectionThresholdUs = 2_000_000`
  (2s); rewrites each video frame's presentation time by `offset` so video is presented on
  the audio timeline. Audio drives the master clock (`AudioRenderer.swift`: `clock.setTimeUs`).

### Why the math is clean (key insight)

For each track, a frame with PTS `P` arriving at monotonic wall time `T` gives
`offset_frame = P − T`; keep the running max `M = max(P − T)`. The track's live edge at any
instant `now` is `now + M`.

At a shared instant `now`, audio live content sits at `now + M_audio` and video at
`now + M_video` — both represent the **same wall-clock capture moment**. So to convert any
video PTS `V` into the audio domain:

```
V_in_audio = V + (M_audio − M_video)
```

The `now` term cancels, so the domain offset is simply **`M_audio − M_video`**. No
synchronized clock is needed between recording and applying — only a running max of
`pts − arrival` per track against a common monotonic origin.

Worked example (the bug): audio PTS ≈ 0 arrives at t≈0 → `M_audio ≈ 0`. Video PTS ≈
5_000_000_000µs arrives at t≈0 → `M_video ≈ 5e9`. Offset = `0 − 5e9 = −5e9`. Aligned video =
`5e9 + (−5e9) = 0` → collapses onto audio's domain. Fixed.

Native / `moq-cli` case: both share an epoch → `M_audio ≈ M_video` → offset ≈ 0 → below the
2s threshold → **not applied** → behaves exactly like today. Small genuine A/V capture skew
(tens of ms) also stays below threshold and is preserved.

## Fix plan for `connection.rs`

Replace the shared `first_pts` + `normalize_pts` with an `Arc<PtsAligner>` that owns per-track
live edges plus a shared base, and route each track's raw PTS through the matching method.

### 1. Add the aligner (new module or inline in `connection.rs`)

```rust
use std::time::Instant;

/// Only remap video into the audio domain when the two epochs diverge by more than this.
/// Mirrors moq-kit VideoRenderer.ptsCorrectionThresholdUs (2s). Below it, natural A/V skew
/// is left untouched and behavior matches the old shared-first-PTS path.
const PTS_ALIGN_THRESHOLD: Duration = Duration::from_secs(2);

#[derive(Default)]
struct LiveEdge {
    /// running max of (pts − arrival), signed microseconds
    max_offset_us: Option<i64>,
}

impl LiveEdge {
    fn record(&mut self, pts: Duration, arrival_us: i64) -> i64 {
        let offset = pts.as_micros() as i64 - arrival_us;
        let m = self.max_offset_us.map_or(offset, |cur| cur.max(offset));
        self.max_offset_us = Some(m);
        m
    }
}

struct PtsAligner {
    /// Common monotonic origin so audio/video arrivals are comparable.
    origin: Instant,
    has_audio: bool,
    audio: Mutex<LiveEdge>,
    video: Mutex<LiveEdge>,
    /// Shared zero point, in the *audio* domain (audio's first raw PTS). Audio is the
    /// reference clock, matching moq-kit (audio drives the master clock).
    base: Mutex<Option<Duration>>,
    /// Latched video→audio domain offset (signed µs). Latched once both edges have a
    /// sample so subsequent video PTS stay monotonic for the decoder.
    video_offset_us: Mutex<Option<i64>>,
}

impl PtsAligner {
    fn new(has_audio: bool) -> Self {
        Self {
            origin: Instant::now(),
            has_audio,
            audio: Mutex::new(LiveEdge::default()),
            video: Mutex::new(LiveEdge::default()),
            base: Mutex::new(None),
            video_offset_us: Mutex::new(None),
        }
    }

    fn arrival_us(&self) -> i64 {
        self.origin.elapsed().as_micros() as i64
    }

    /// Audio is the reference domain: it establishes the shared base and rebases to ~0.
    fn normalize_audio(&self, raw_pts: Duration) -> Duration {
        let arrival = self.arrival_us();
        self.audio.lock().unwrap().record(raw_pts, arrival);
        let base = *self.base.lock().unwrap().get_or_insert(raw_pts);
        raw_pts.saturating_sub(base)
    }

    /// Video is remapped into the audio domain via the live-edge offset, then rebased by
    /// the shared audio base.
    fn normalize_video(&self, raw_pts: Duration) -> Duration {
        let arrival = self.arrival_us();
        let m_video = self.video.lock().unwrap().record(raw_pts, arrival);

        // Video-only input: video is its own reference (matches "video sets first_pts").
        if !self.has_audio {
            let base = *self.base.lock().unwrap().get_or_insert(raw_pts);
            return raw_pts.saturating_sub(base);
        }

        // Latch the domain offset once audio has produced an edge sample.
        let offset_us = {
            let mut latched = self.video_offset_us.lock().unwrap();
            if latched.is_none() {
                if let Some(m_audio) = self.audio.lock().unwrap().max_offset_us {
                    let raw = m_audio - m_video;
                    // Threshold guard: ignore sub-2s divergence (natural skew / shared epoch).
                    let applied = if raw.unsigned_abs() as u128
                        > PTS_ALIGN_THRESHOLD.as_micros()
                    {
                        raw
                    } else {
                        0
                    };
                    *latched = Some(applied);
                }
            }
            (*latched).unwrap_or(0)
        };

        let aligned_us = raw_pts.as_micros() as i64 + offset_us;
        let aligned = Duration::from_micros(aligned_us.max(0) as u64);
        let base = *self.base.lock().unwrap().get_or_insert(aligned);
        aligned.saturating_sub(base)
    }
}
```

### 2. Wire it into `TrackCtx` / `BroadcastHandler`

- In `TrackCtx` (`connection.rs:54-63`), replace
  `first_pts: Arc<Mutex<Option<Duration>>>` with `aligner: Arc<PtsAligner>`.
- In `BroadcastHandler::new` (`connection.rs:161-192`), replace the shared `first_pts`
  construction with `let aligner = Arc::new(PtsAligner::new(audio.is_some()));`
  (compute `has_audio` from the `audio` argument before moving it into `self`).
- In `run_video_track` (`connection.rs:283`), replace
  `let pts = normalize_pts(&first_pts, raw_pts);` with
  `let pts = aligner.normalize_video(raw_pts);`.
- In `run_audio_track` (`connection.rs:346`), replace with
  `let pts = aligner.normalize_audio(raw_pts);`.
- Delete `normalize_pts` (`connection.rs:514-520`) and its doc comment. Update the
  destructuring in both `run_*_track` (`connection.rs:261`, `:325`) to pull `aligner`
  instead of `first_pts`.

The `QueueTrackOffset::Pts(effective_last_pts + MOQ_BUFFER)` at `connection.rs:135-139`
stays as-is — both tracks still emit a zero-based, common-domain PTS, so the existing queue
offset keeps placing the input on smelter's timeline correctly.

## Edge cases and implementation notes

- **Reference domain = audio.** Audio sets the shared base and drives the zero point, mirroring
  moq-kit where audio drives the master clock. Video is always the one remapped.
- **Video frame before any audio frame.** In the reported bug audio always leads, so
  `M_audio` and the base exist by the time video arrives. If an input can deliver video first,
  `normalize_video` will latch `offset = 0` until audio produces its first edge sample, which
  can misalign the earliest video frames. Recommended hardening: briefly hold video frames
  (bounded by `MOQ_BUFFER`) until `audio.max_offset_us` is `Some`, then latch and flush. Only
  add this if a video-first publisher is in scope.
- **Monotonic PTS for the decoder.** The offset is **latched once** rather than recomputed
  per frame (moq-kit recomputes per frame because it retimes for display, not decode). Latching
  keeps `raw_video + constant` monotonic into the H264/VP8/VP9 decoder threads. Optional
  refinement: warm up over the first few frames of each track (bounded by `MOQ_BUFFER`) before
  latching, so a single late-arriving first frame can't skew `M`.
- **Threshold.** Keep 2s to match moq-kit and to make the native / `moq-cli` path a no-op
  (offset falls below threshold → 0 → identical to today's shared normalization).
- **Video-only / audio-only.** `has_audio == false` makes video its own reference; an
  audio-only input never calls `normalize_video`. Both reproduce the current correct behavior.

## Reconcile with existing A/V sync (`server/smelter-core/src/queue/`)

The shared `first_pts` existed to preserve inter-track sync via a common zero. The live-edge
offset preserves that relationship more accurately: it aligns the two tracks by their live
edges (same wall-clock capture moment) instead of assuming their first frames were
simultaneous. Confirm the queue's `QueueTrackOffset` / `TrackOffset` still receives a single
common-domain timeline from both tracks — it does, because audio and video both come out
zero-based in the audio domain. No queue changes expected.

## Validation

1. **Repro:** publish audio+video from `tools/src/tools/MoqStreamer.tsx` → smelter. Before the
   fix: audio-only. After: both, in sync. Test CMAF / Legacy / LOC and each codec (the defect
   is epoch-only, so all should pass).
2. **Regression:** `moq-cli` fMP4 publish (shared epoch) must be unchanged — verify the offset
   latches to 0 (below threshold). Trace logs at `connection.rs:284`/`:347` show
   `pts` ≈ `raw_pts − base` as before.
3. **Video-only** and **audio-only** inputs still play.
4. **Lip-sync:** eyeball A/V alignment on the browser publisher after the fix; the live-edge
   offset should land audio and video within natural skew.
5. Use `/verify` after implementing to confirm the drop is gone and sync/video-only paths are
   intact.

## Source references

- Bug site: `server/smelter-core/src/pipeline/moq/connection.rs`
  — shared `first_pts` `:60,:174`; `normalize_pts` `:516`; call sites `:283,:346`.
- Method to port (moq-kit repo):
  - `ios/Sources/MoQKit/Subscribe/internal/playback/MediaLiveEdge.swift` (per-track running max)
  - `ios/Sources/MoQKit/Subscribe/internal/playback/MediaTimestampAligner.swift` (domain offset + threshold)
  - `ios/Sources/MoQKit/Subscribe/internal/playback/VideoRenderer.swift` (2s threshold, apply offset)
  - `ios/Sources/MoQKit/Subscribe/internal/playback/AudioRenderer.swift` (audio drives clock)
  - Android equivalents under `android/moqkit/.../playback/`.
- Note: moq-kit's **publisher** (`ios/.../Publish/Clock.swift`) uses the same single-shared-epoch
  pattern that breaks in smelter — safe there only because native capture stamps A/V from one
  host clock. The lesson is exactly to move robustness to the consumer/input side, which is
  what this fix does.
