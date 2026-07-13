# More robust epoch-discontinuity detection (offset-space)

## Context

`is_epoch_discontinuity` (`timestamp_aligner.rs:400`) currently flags a mid-stream
epoch change when a **keyframe's raw PTS jumps > `MOQ_EPOCH_DISCONTINUITY` (500 ms)**
from the previous frame's raw PTS. On a hit, `connection.rs` calls `estimator.reset()`,
which drops the locked offset and re-locks at the current live edge (`reset()` keeps
`first_epoch = false`, so the anchor/small-skew branch is skipped on re-lock).

Problem: a raw-PTS jump conflates two distinct events:
- **Real epoch change** (publisher restart / clock switch) — reset is correct.
- **Same-epoch content gap** (a stalled group given up after `MOQ_BUFFER`, dropped
  frames) — reset is **wrong**. It shifts the timeline and, worse, **permanently
  downgrades an anchor/small-skew lock to a live-edge lock**, breaking cross-epoch
  A/V alignment (the two tracks can re-lock against different references).

The distinguishing signal already exists in the code: the per-frame **offset
`raw − elapsed`** (`EpochOffset::new`, line 39). `Δoffset = Δraw − Δelapsed`, so the
test asks "did raw advance faster than wall-clock?". Across a same-epoch drop under live
delivery `raw` and `elapsed` advance together (the skipped content streamed past in real
time), so the offset is ~unchanged. Across a real epoch change `raw` jumps while
`elapsed` does not, so the offset steps by ~the jump. So compare **consecutive-frame
offsets**, not raw PTS. (See "Consumer skip mechanism" for the exact bound.)

Intended outcome: dropped frames no longer trigger a reset; genuine epoch changes
(forward or backward) still do.

## Hard constraint: monotonic output, immutable shared anchor

Timestamps fed to the queue MUST be monotonic. The shared anchor (`EpochShared`,
`OnceLock`) therefore stays **immutable** — re-anchoring mid-stream could emit a PTS
that steps backward. Consequences:
- `reset()` stays **loop-local** (per-track) and re-locks to **live-edge**, because
  `normalized = raw − (raw − elapsed) ≈ elapsed` continues from ever-growing wall-clock
  and is the only monotonic-safe re-lock. No shared-state mutation, no generation
  counter, no re-run of `skew_decision`.
- No reset guard (cross-track confirmation / anchor-preserving reset both need
  added/mutable shared state): the residual false-positive is **accepted**. Both test
  branches preserve monotonicity — not resetting on a drop leaves a *forward* gap;
  resetting on an epoch change collapses to live-edge — so the residual is at worst a
  rare, bounded A/V-sync glitch, never a monotonicity break.

## Approach

Move detection into the estimator so `EpochOffset` and `elapsed` stay encapsulated and
a single `elapsed` read feeds both the check and normalization.

**`timestamp_aligner.rs`:**
1. Add field `last: Option<(Duration, EpochOffset)>` (previous `(raw_pts, offset)`) to
   `LiveEdgeEstimator` (init `None`). Set every frame (locked or warming). `reset()` does
   **not** clear it — the post-jump frame becomes the next baseline.
2. Rewrite the predicate (pure, unit-testable) with a keyframe gate + three ordered
   branches on the raw-PTS step, only reaching the offset delta as a tie-breaker:
   ```rust
   fn is_epoch_discontinuity(
       keyframe: bool,
       raw_pts: Duration,
       offset: EpochOffset,
       last: Option<(Duration, EpochOffset)>, // (last_raw_pts, last_offset)
   ) -> bool {
       if !keyframe { return false; }
       let Some((last_raw_pts, last_offset)) = last else { return false; };
       // 1. Small forward step → normal cadence, not an epoch change.
       if raw_pts >= last_raw_pts && raw_pts - last_raw_pts < MOQ_EPOCH_MIN_STEP {
           return false;
       }
       // 2. Time went backwards → clock reset / new epoch.
       if raw_pts < last_raw_pts { return true; }
       // 3. Forward jump ≥ MOQ_EPOCH_MIN_STEP → disambiguate by offset delta.
       offset.abs_diff(last_offset) > MOQ_EPOCH_OFFSET_JUMP
   }
   ```
   Branches are mutually exclusive/exhaustive (branch 3 sees only `Δraw ≥ 100ms`, no
   underflow). Reuse `EpochOffset::abs_diff` (line 56) for the signed/cross-sign case.
   New constant `MOQ_EPOCH_MIN_STEP: Duration = Duration::from_millis(100)`.
   Backward-jump branch relies on keyframe-PTS monotonicity across GOPs — safe because
   the keyframe gate compares a group's first frame to the previous group's last.
3. Add `on_frame(&mut self, keyframe: bool, chunk) -> Vec<EncodedInputChunk>`
   (public) + `on_frame_at(&mut self, keyframe, elapsed, chunk)` (clock-injected core,
   mirroring the existing `on_chunk` / `on_chunk_at` split at lines 230/236). Flow:
   read `elapsed` once → `offset = EpochOffset::new(chunk.pts, elapsed)` →
   if `is_epoch_discontinuity(keyframe, chunk.pts, offset, self.last)` then `reset()` →
   store `self.last = Some((chunk.pts, offset))` → delegate to `on_chunk_at(elapsed,
   chunk)` (reusing the same `elapsed`). Tracked on **every** frame, including locked.
   Keep the **keyframe gate** in the predicate: `moq-mux` sets `keyframe = true` at
   group starts, and audio publishers mark every Nth frame as a keyframe to close a
   group (`moq-mux container/mod.rs:55-62`, `producer.rs:16-18`). A real epoch change
   begins a new group → keyframe on both tracks, so the gate still catches it while
   aligning the reset to a decodable/group boundary and filtering mid-GOP noise.
4. Set the offset-jump threshold to **`AV_SKEW_MAX` (2 s)** — define a dedicated
   constant equal to `AV_SKEW_MAX` (e.g. rename `MOQ_EPOCH_DISCONTINUITY` →
   `MOQ_EPOCH_OFFSET_JUMP = AV_SKEW_MAX`) so the raw-delta→offset-delta semantic change
   is explicit and the two stay tunable independently. Rationale: both constants express
   the same "how far apart before it's a different epoch" scale — offset shifts within
   A/V-skew tolerance are normal cross-track wobble, not a new epoch. Interaction to
   note: 2 s is just below `MOQ_BUFFER` (2.2 s), so the accepted residual (already-
   buffered bursty skip) now sits right at the buffer scale — a ~2.2 s buffered-skip step
   can exceed 2 s and re-lock to live edge. Does not occur under live real-time delivery.

**`connection.rs` (`run_video_track` ~296–332, `run_audio_track` ~385–420):**
5. Delete the `last_raw_pts` bookkeeping and the inline
   `is_epoch_discontinuity(...) / reset()` block. Build the chunk, then call
   `estimator.on_frame(frame.keyframe, chunk)`. Keep the existing `debug!` reset log by
   moving it behind the estimator (e.g. estimator returns/logs, or log inside `on_frame`).

**Tests (`timestamp_aligner.rs:680–716`):**
6. Rewrite `reset_on_epoch_jump_relocks_live_edge` and the `is_epoch_discontinuity`
   unit cases to drive `on_frame_at` / the offset-space predicate with injected
   `elapsed`. Add cases that lock the distinction:
   - **Small forward step (branch 1):** raw +33 ms (< 100 ms) → **no reset**, even if
     offset would otherwise look off (short-circuits before the offset check).
   - **Backward jump (branch 2):** any keyframe with raw < last_raw → **reset**,
     regardless of magnitude/offset.
   - **Drop, same epoch (branch 3, offset small):** raw +2 s **and** elapsed +2 s →
     offset stable → **no reset**.
   - **Epoch jump (branch 3, offset large):** raw +50 s, elapsed unchanged → **reset**.
   - **Burst warmup:** many frames, each consecutive-frame step small → **no false
     reset** mid-warmup.
   - **Keyframe gate:** a non-keyframe (even with a large raw/offset step) → **no reset**.
   - Preserve the existing "anchor lock survives a drop" intent: an anchor/small-skew
     locked track hit by a same-epoch drop keeps its `locked_offset`.

## Consumer skip mechanism (why the premise holds, and its bound)

`moq-mux` `Consumer::poll_read` (`consumer.rs:163-186`) skips a stalled group in
**timestamp-space, not via a wall-clock timer**: it drops the current group when newer
*buffered* groups lead it by ≥ `latency` (`MOQ_BUFFER` = 2200 ms) in PTS
(`max_timestamp.saturating_sub(oldest) >= self.latency`). So the offset stays stable
across a drop only because, under **live real-time delivery**, `Δelapsed ≈ Δraw` while
the newer groups stream in. The algebra: `Δoffset = Δraw − Δelapsed` — the test asks
"did raw advance faster than wall-clock?", which the old raw-only test ignored.

## Known limitation (accepted)

If the skip crosses content that is **already buffered** (a burst/backlog arrived, e.g.
after a reconnection), the skip happens with little `Δelapsed`, so `Δoffset ≈ Δraw` and
a same-epoch large gap can still false-positive → live-edge re-lock. Smooth-burst
frame-to-frame steps stay tiny (~one frame interval) since the compare is
consecutive-frame; only a single skip over a large buffered gap misfires.

## Verification

- `cargo test -p smelter-core moq::connection::timestamp_aligner` (or the module path)
  — all rewritten/added cases pass.
- `cargo clippy -p smelter-core` clean.
- Manual/logging: confirm a dropped-group scenario no longer emits the
  "epoch discontinuity detected, resetting estimator" `debug!` while a simulated
  publisher restart still does.
