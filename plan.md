# Shared sync-mode decision for MoQ timestamp aligner

## Context

`TimestampAligner` (`smelter-core/src/pipeline/moq/connection/timestamp_aligner.rs`) decides per track whether to anchor to the shared first timestamp (small A/V skew) or lock at the track's own live edge (large skew). The `Pending`-deadline branch (`timestamp_aligner.rs:313-318`) has an inconsistency: if one track starts >1s late (e.g. browser publisher with a muted/permission-delayed mic), the early track deadline-locks at its own **live edge** (`max_offset`), but the late track — measuring small skew — anchors to `anchor_offset` (the early track's *first-frame* offset). The two tracks then normalize with different constants, producing a fixed A/V desync up to the early track's warmup burst climb (relay group backfill, potentially seconds).

A related latent bug: an epoch discontinuity on one track *before* the counterpart's first frame leaves the set-once `first_offset` pointing at a dead epoch; the late counterpart can then measure "small skew" against stale data and anchor while the other track is live-edge locked on a new epoch.

**Decided fix** (from design discussion): share the *mode decision* itself. Once any track decides the stream's sync mode, the other track follows it — including "live edge even though my measured skew is small." An epoch discontinuity while the mode is undecided forces `LiveEdge`, since a discontinuity is direct proof of a multi-epoch publisher.

Accepted trade-off: in the deadline + late-counterpart + genuinely-small-skew case, alignment degrades from exact-by-construction to the live-edge estimation residual — the same documented limitation already accepted for large-skew streams. In exchange the invariant is crisp: *anchor only when a single-epoch first-frame pair was verified in time; otherwise the whole stream is live-edge.*

## Changes

All in `smelter-core/src/pipeline/moq/connection/timestamp_aligner.rs`. No API/type changes visible outside the module (`connection.rs` untouched).

### 1. New shared mode enum + `EpochShared` field

```rust
/// The stream-wide sync mode, decided once by whichever track (or event)
/// resolves it first; the other track follows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SyncMode {
    /// Verified single-epoch: lock immediately at the shared anchor offset.
    Anchor,
    /// Per-track live-edge estimation (large skew, deadline with counterpart
    /// unseen, or epoch discontinuity before the decision).
    LiveEdge,
}
```

- Add `mode: Arc<OnceLock<SyncMode>>` to `EpochShared` (init in `EpochShared::new`).
- Add commit-or-adopt helper so races between tracks resolve atomically:

```rust
/// Commit `mode` as the stream's sync mode, or adopt the mode already
/// decided by the other track / a discontinuity (set-once).
fn decide_mode(&self, mode: SyncMode) -> SyncMode {
    *self.mode.get_or_init(|| mode)
}
```

plus a read accessor `fn mode(&self) -> Option<SyncMode>`.

### 2. Rework the first-epoch branch of `advance_warmup`

Replace the `SkewDecision` match (`timestamp_aligner.rs:300-326`) with: resolve the mode first (shared value wins; otherwise measure), then act uniformly per mode.

Mode resolution, replacing `skew_decision()`:
1. Shared mode already set → follow it (no skew measurement).
2. `single_track_stream` → `decide_mode(Anchor)`.
3. Counterpart's `first_offset` present → measure skew; `decide_mode(Anchor)` if ≤ `AV_SKEW_MAX`, else `decide_mode(LiveEdge)`.
4. Counterpart unseen and `warmup_deadline_passed` → `decide_mode(LiveEdge)`.
5. Counterpart unseen, deadline not passed → undecided; hold the frame.

Acting on the resolved mode:
- `Anchor` → `lock_and_flush(shared.anchor_offset())` immediately (anchor offset is guaranteed set — this track's own first frame set it before reaching here).
- `LiveEdge` → existing warmup machinery: `live_edge_settled(elapsed, started)` → `lock_and_flush(max_offset)`, else hold. Note the deadline path needs no special lock anymore: it decides `LiveEdge`, and `live_edge_settled` fires the same frame because the deadline has passed — one uniform path.

Cleanup enabled by this:
- Delete the per-track `skew_decided: bool` field — "shared mode is set" carries the same information.
- Delete the `SkewDecision` enum and `skew_decision()`; `Pending` becomes simply "mode unresolved."

### 3. Discontinuity hook in `on_chunk`

In the `is_epoch_discontinuity` branch (`timestamp_aligner.rs:252-255`), before/alongside `reset()`:

```rust
// A discontinuity proves the publisher is not single-epoch, so a
// counterpart that has not decided yet must never anchor against the
// stale first offsets. No-op if the mode is already decided.
_ = self.shared.mode.set(SyncMode::LiveEdge);
```

(or via `decide_mode`, ignoring the return value).

### 4. Unchanged behavior — verify these still hold

- **Post-reset epochs**: `first_epoch = false` skips the mode block entirely; always per-track live-edge via `live_edge_settled`. Mode is a first-epoch-only concept.
- **`flush()` (EOS force-lock)**: stays as-is; does not touch the shared mode (it can run mid-warmup of a later epoch where the mode is irrelevant).
- **Set-once shared state**: wall-clock anchor, `anchor_offset`, per-track first offsets, and now the mode never reset for the connection's lifetime.

### 5. Comment updates

- `EpochShared` doc: describe the mode field ("decided once, followed by both tracks").
- `TimestampAligner` struct doc: reword the decision flow — anchor only on a verified single-epoch first-frame pair; deadline/discontinuity/large-skew all resolve to live-edge.
- Note at the mode-resolution site that the two measured branches always agree between tracks (same set-once first offsets); the deadline and discontinuity setters are the only ones adding new information.

## Verification

Unit tests for this file were deliberately removed (commit 55b9d1db), so verification is compile + review + manual:

1. `cargo check -p smelter-core` and `cargo clippy -p smelter-core` (note: HEAD contains a deliberate compilation error commit — verify against the working tree).
2. Walk the decision table against the scenarios from the discussion:
   - both tracks prompt, small skew → both `Anchor` at `anchor_offset` (unchanged fast path)
   - large skew → both `LiveEdge` (unchanged)
   - counterpart >1s late, small skew → early track decides `LiveEdge` at deadline; late track **follows** `LiveEdge` (the bug fix)
   - discontinuity before counterpart's first frame → mode forced `LiveEdge`; late track never anchors to the stale epoch
   - single-track stream → `Anchor` (unchanged)
   - mid-stream discontinuity after lock → per-track live-edge re-warm (unchanged)
3. Manual smoke test with a MoQ publisher if available (e.g. video+audio publish, then a publish where audio starts several seconds late) checking A/V sync at the output.
