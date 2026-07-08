# MoQ Live-Edge PTS Alignment — Type & Constant Reference

Reference for the types and constants added to
`smelter-core/src/pipeline/moq/connection.rs` when porting moq-kit's "live edge"
epoch alignment. This replaces the old single shared `first_pts` anchor that
dropped video whenever a publisher put audio and video on different timestamp
epochs (the browser-publisher case).

## The problem in one line

A browser publisher emits audio on a ~0-based epoch and video on a large
capture-clock epoch. The old code normalized both tracks against whichever frame
arrived first, pushing video ~20s into the future → the queue dropped it all →
audio-only playback. The fix estimates a **per-track** epoch offset against **one
shared wall-clock**, so both tracks land on the same timeline and stay in sync.

---

## Constants

| Const | Value | Role |
| --- | --- | --- |
| `EPOCH_RECONCILE_EPSILON` | 50 ms | Two per-track offsets within this distance are treated as the **same** PTS epoch. The second track to lock adopts the shared reference → exact A/V sync for single-epoch publishers (`moq-cli`). Beyond it, each track keeps its own offset (browser cross-epoch case). Sits in a wide safe band: same-epoch noise ≪ 50 ms ≪ seconds-scale real epoch gaps. |
| `MOQ_EPOCH_MAX_WARMUP` | 1 s | Fallback lock deadline. If the running max never plateaus (stream trickles in without a startup burst — publisher just went live, or a sparse/low-fps track), lock anyway after this much elapsed time so frames aren't held forever. |
| `PLATEAU_FRAMES` | 3 | Number of consecutive frames that must fail to raise the running max before we consider the startup burst drained (live edge reached) and lock. The normal, low-latency lock path (locks in ~ms on a bursty start). |
| `PLATEAU_EPSILON` | 5 ms | Tolerance for "the running max did not rise" when counting plateau frames. A frame that lifts the max by ≤ this counts toward the plateau; more than this resets the plateau counter to 0. |
| `MOQ_EPOCH_DISCONTINUITY` | 500 ms | A **keyframe** whose raw PTS jumps by more than this from the previous frame is treated as a mid-stream epoch discontinuity, triggering an estimator `reset()`. Mirrors moq-kit's `discontinuityGapUs`. |

Pre-existing constants (unchanged, for context): `MOQ_BUFFER` (1 s container
read latency) and `MOQ_MAX_BUFFER` (20 s decoder/queue buffer — the window that
was dropping video).

---

## Types

### `EpochOffset`

```rust
struct EpochOffset { magnitude: Duration, negative: bool }
```

A **signed** offset `raw_pts − elapsed` — a track's raw PTS measured at the shared
anchor instant. Stored as a `Duration` magnitude plus a sign rather than raw i64
micros (Rust `Duration` is unsigned, and the offset can legitimately go negative
when a near-zero-epoch track is first observed *after* another track already set
the anchor).

Key operations:
- `new(raw, elapsed)` — builds the signed offset, picking the sign from whether
  `raw ≥ elapsed`.
- `normalize(raw)` → `raw − self`. This is the actual PTS the estimator emits.
  Saturates at zero on the positive branch (never emits a negative Duration).
- `abs_diff(other)` → `|self − other|`, used for the reconciliation epsilon and
  the plateau "did the max rise?" check.
- `Ord` — ordering is defined so **less-late is greater**: positive > negative,
  and among negatives the one closer to zero is greater. This makes the running
  `max` of `raw − elapsed` converge to the true live edge (frames only ever
  arrive *late*, so every sample is ≤ the true edge; the max climbs from below
  with no overshoot).

### `EpochShared`

```rust
struct EpochShared {
    anchor: Arc<OnceLock<Instant>>,
    reference_off: Arc<OnceLock<EpochOffset>>,
}
```

The state **shared between the audio and video track tasks**. Cloned into each
track (both `Arc`-backed `OnceLock`s, so both tasks see the same cell).

- `anchor` — the single monotonic wall-clock zero point, set once on the first
  frame from **any** track via `elapsed()` (`get_or_init(Instant::now)`). Because
  both tracks measure their offset against this same instant, the shared `now`
  cancels out and the difference between the two tracks' offsets is exactly their
  epoch difference — that's what makes A/V auto-align.
- `reference_off` — the reconciliation reference. Set once by the **first track
  to lock** (`OnceLock` makes the near-simultaneous race well-defined: first
  writer wins, no barrier, no added latency). The second track compares against
  it (see `EPOCH_RECONCILE_EPSILON`).
- `elapsed()` — `Duration` since the anchor, initializing the anchor on first
  call.

### `LiveEdgeEstimator`

```rust
struct LiveEdgeEstimator {
    shared: EpochShared,
    warmup: Duration,
    started_elapsed: Option<Duration>,
    max_off: Option<EpochOffset>,
    plateau_frames: u32,
    held: Vec<EncodedInputChunk>,
    locked_off: Option<EpochOffset>,
    reconciled: bool,
}
```

Per-track, loop-local. This is the port of moq-kit's `MediaLiveEdge`. It estimates
the track's PTS epoch at the shared anchor as the **running max of `raw − elapsed`**,
holds frames until the estimate stabilizes, then **locks a single constant offset**
and streams the rest. Locking (rather than moq-kit's continuous re-derivation) is
the one deliberate divergence — smelter normalizes *before* the decoder and feeds a
monotonic queue, so it can't re-shift an already-emitted PTS. Because frames are
held only until the max plateaus (~ms), the locked constant applies from the first
*emitted* frame ⇒ output is monotonic by construction.

Fields:
- `shared` — the `EpochShared` above (anchor + reconciliation reference).
- `warmup` — the fallback lock deadline (`MOQ_EPOCH_MAX_WARMUP`), measured against
  the shared elapsed clock (not a second wall clock).
- `started_elapsed` — shared elapsed at the first observed frame; the base for the
  warmup-deadline comparison.
- `max_off` — the running max of `raw − elapsed`; equals the live-edge offset.
- `plateau_frames` — consecutive frames that didn't raise `max_off` by more than
  `PLATEAU_EPSILON`. Reaching `PLATEAU_FRAMES` means the burst drained → lock.
- `held` — frames buffered during warmup, each carrying its raw PTS in `chunk.pts`.
  Drained and normalized at lock. This is ~ms of real wall-clock ⇒ ≈ zero added
  latency.
- `locked_off` — `Some` once locked; every subsequent frame is normalized with it.
- `reconciled` — whether §0 reconciliation has run. Set true on first lock and
  **kept true across `reset()`**, so a re-lock after a discontinuity uses the
  track's own re-derived offset instead of the stale (original-epoch) reference.

Methods:
- `new(shared, warmup)` — construct in the warming-up state.
- `on_chunk(chunk)` — production entry point; reads the shared clock, delegates to
  `on_chunk_at`.
- `on_chunk_at(elapsed, chunk)` — clock-injected core (this is what the unit tests
  drive with synthetic `(raw, elapsed)` sequences, no real sleeps). Returns the
  chunks ready to emit: **empty** while warming (chunk held), the **full flushed
  batch** at the moment of lock, or the **single normalized chunk** once locked.
- `lock_and_flush(max_off)` — lock at the given offset (running the reconciliation
  against `reference_off` on the first lock only), then drain and normalize all
  held frames.
- `flush()` — EOS force-lock-and-flush: locks at the current running max and
  drains held frames so a sub-warmup clip still renders. No-op if already locked
  (held is empty) or if no frame was ever received.
- `reset()` — mid-stream discontinuity reset (moq-kit's `reset()`). Clears
  `locked_off`, `max_off`, `plateau_frames`, `started_elapsed` so the estimator
  re-warms and re-locks against the **same, never-reset** shared anchor — the
  re-derived offset absorbs the input jump and keeps normalized output continuous.
  Does **not** clear `held` (empty while locked) or `reconciled`.
- `locked_off()` — `#[cfg(test)]` accessor for the locked offset.

### `is_epoch_discontinuity` (free function)

```rust
fn is_epoch_discontinuity(keyframe: bool, raw_pts: Duration, last_raw_pts: Option<Duration>) -> bool
```

The discontinuity predicate shared by both read loops: `keyframe && |raw − last| >
MOQ_EPOCH_DISCONTINUITY`. Non-keyframes and the very first frame never trigger.
Factored out so the audio and video loops stay DRY and the condition is unit-tested
directly.

---

## How they fit together (per frame)

1. Read loop pulls a frame, checks `is_epoch_discontinuity` → maybe `reset()`.
2. Builds an `EncodedInputChunk` with `pts = raw_pts` (raw) and calls
   `estimator.on_chunk(chunk)`.
3. `EpochShared::elapsed()` sets/reads the anchor; `EpochOffset::new(raw, elapsed)`
   is this frame's vote for the epoch offset.
4. While warming: push to `held`, update `max_off`, count `plateau_frames`.
5. At lock (plateau or warmup deadline): reconcile against `reference_off`, set
   `locked_off`, flush `held` normalized with the locked offset.
6. After lock: each frame is `EpochOffset::normalize(raw)`d and emitted directly.
7. On EOS (`consumer.read()` → `None`): `estimator.flush()` before sending `EOS`.
   On teardown / send failure: drop held frames and break.

Result: audio (arrives first, ~0 epoch) normalizes to ~0; video (arrives `d` later
on a large epoch) normalizes to ≈ `d`, correctly placed later on the same timeline.
Both track wall-clock, so they stay in sync.

## Tests

`connection.rs` has a `#[cfg(test)] mod tests` (10 tests) driving `on_chunk_at`
with injected clocks: `EpochOffset` arithmetic/ordering, steady-stream lock,
burst-drain lock, EOS force-flush, cross-epoch ~300 ms alignment, same-epoch
reconciliation, distant-epoch keep-own, discontinuity reset continuity, and the
detection conditions.

## Still open

End-to-end `/verify` (browser publisher from `tools/src/tools/MoqStreamer.tsx` +
`moq-cli` regression) — needs the live MoQ server, not yet run.
