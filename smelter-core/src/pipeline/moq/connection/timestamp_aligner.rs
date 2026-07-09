use std::{
    cmp::Ordering,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use crate::prelude::*;

/// Two per-track offsets within this distance are treated as the same PTS epoch,
/// so the second track adopts the shared reference for exact A/V alignment
/// (single-epoch publishers such as `moq-cli`). Beyond it, each track keeps its
/// own offset (the browser cross-epoch case).
const EPOCH_RECONCILE_EPSILON: Duration = Duration::from_millis(50);
/// Fallback lock deadline for streams that trickle in without a startup burst
/// (publisher just went live, sparse/low-fps tracks).
const MOQ_EPOCH_MAX_WARMUP: Duration = Duration::from_secs(1);
/// Consecutive frames that fail to raise the running max by more than
/// [`PLATEAU_EPSILON`] before we consider the startup burst drained (live edge
/// reached) and lock.
const PLATEAU_FRAMES: u32 = 3;
/// Tolerance for "the running max did not rise" when counting plateau frames.
const PLATEAU_EPSILON: Duration = Duration::from_millis(5);
/// A keyframe whose raw PTS jumps by more than this from the previous frame is
/// treated as a mid-stream epoch discontinuity, resetting the estimator.
const MOQ_EPOCH_DISCONTINUITY: Duration = Duration::from_millis(500);

/// Signed offset `raw_pts − elapsed` (a track's raw PTS at the shared anchor
/// instant), kept as a [`Duration`] magnitude plus a sign — no raw i64 micros.
/// Negative when a track's near-zero raw PTS is first observed well *after*
/// another track set the anchor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EpochOffset {
    magnitude: Duration,
    negative: bool,
}

impl EpochOffset {
    fn new(raw: Duration, elapsed: Duration) -> Self {
        Self {
            magnitude: raw.abs_diff(elapsed),
            negative: raw < elapsed,
        }
    }

    /// normalized PTS = `raw − self`
    fn normalize(self, raw: Duration) -> Duration {
        if self.negative {
            raw + self.magnitude
        } else {
            raw.saturating_sub(self.magnitude)
        }
    }

    /// `|self − other|`, for the reconciliation / plateau epsilon checks.
    fn abs_diff(self, other: Self) -> Duration {
        if self.negative == other.negative {
            self.magnitude.abs_diff(other.magnitude)
        } else {
            self.magnitude + other.magnitude
        }
    }
}

impl Ord for EpochOffset {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.magnitude.cmp(&other.magnitude),
            (true, true) => other.magnitude.cmp(&self.magnitude), // less-negative is greater
        }
    }
}

impl PartialOrd for EpochOffset {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Shared across both track tasks: the wall-clock anchor (set once on the first
/// frame from ANY track) and the reconciliation reference (set once by the first
/// track to lock).
#[derive(Clone)]
pub(super) struct EpochShared {
    anchor: Arc<OnceLock<Instant>>,
    reference_off: Arc<OnceLock<EpochOffset>>,
}

impl EpochShared {
    pub fn new() -> Self {
        Self {
            anchor: Arc::new(OnceLock::new()),
            reference_off: Arc::new(OnceLock::new()),
        }
    }

    /// Elapsed since the shared anchor, initializing the anchor on the first call
    /// from any track.
    pub fn elapsed(&self) -> Duration {
        self.anchor.get_or_init(Instant::now).elapsed()
    }
}

/// Per-track, loop-local. Estimates the track's PTS epoch at the shared anchor via
/// the live edge: the running max of `raw − elapsed`,
/// locked once the max plateaus (startup burst drained) or a fallback deadline
/// fires. Frames are held until lock (~ms of real wall-clock, just the burst
/// window) so the locked constant applies from the first *emitted* frame and
/// output is monotonic by construction.
///
/// Latency-skew assumption: edge sync aligns each track's newest-available sample
/// to "now", so materially different per-track transport latency leaves a fixed
/// residual A/V skew. This is inherent to edge-based sync;
/// there is no on-wire capture-time signal to do better without a publisher change.
pub(super) struct LiveEdgeEstimator {
    shared: EpochShared,
    /// Shared elapsed at the first observed frame (warmup start); `None` until then.
    started_elapsed: Option<Duration>,
    /// Running max of `raw − elapsed`; equals the live-edge offset.
    max_off: Option<EpochOffset>,
    /// Consecutive frames that did not raise the max by more than [`PLATEAU_EPSILON`].
    plateau_frames: u32,
    /// Frames buffered until lock; each carries its raw PTS in `chunk.pts`.
    held: Vec<EncodedInputChunk>,
    locked_off: Option<EpochOffset>,
    /// §0 reconciliation runs on the first lock only (`reset()` keeps this `true`).
    reconciled: bool,
}

impl LiveEdgeEstimator {
    pub fn new(shared: EpochShared) -> Self {
        Self {
            shared,
            started_elapsed: None,
            max_off: None,
            plateau_frames: 0,
            held: Vec::new(),
            locked_off: None,
            reconciled: false,
        }
    }

    /// Feed one chunk (with its raw PTS in `chunk.pts`). Returns the chunks ready
    /// to emit: empty while warming (chunk held), the full flushed batch at lock,
    /// or the single normalized chunk once locked.
    pub fn on_chunk(&mut self, chunk: EncodedInputChunk) -> Vec<EncodedInputChunk> {
        let elapsed = self.shared.elapsed();
        self.on_chunk_at(elapsed, chunk)
    }

    /// Clock-injected core of [`on_chunk`], for testing without real sleeps.
    fn on_chunk_at(
        &mut self,
        elapsed: Duration,
        mut chunk: EncodedInputChunk,
    ) -> Vec<EncodedInputChunk> {
        let raw = chunk.pts;

        if let Some(off) = self.locked_off {
            chunk.pts = off.normalize(raw);
            return vec![chunk];
        }

        // Warming up: the running max is the live edge. Frames only ever arrive
        // late, so `off <= edge` and the max climbs from below with no overshoot;
        // it plateaus once the burst drains.
        let off = EpochOffset::new(raw, elapsed);
        let prev = self.max_off;
        let max_off = prev.map_or(off, |p| p.max(off));
        self.max_off = Some(max_off);
        if prev.is_some_and(|p| max_off.abs_diff(p) <= PLATEAU_EPSILON) {
            self.plateau_frames += 1;
        } else {
            self.plateau_frames = 0;
        }
        self.held.push(chunk);

        let started = *self.started_elapsed.get_or_insert(elapsed);
        if self.plateau_frames >= PLATEAU_FRAMES
            || elapsed.saturating_sub(started) >= MOQ_EPOCH_MAX_WARMUP
        {
            return self.lock_and_flush(max_off);
        }
        Vec::new()
    }

    /// Lock at the given offset (reconciled on first lock per §0) and return all
    /// held chunks normalized with it.
    fn lock_and_flush(&mut self, max_off: EpochOffset) -> Vec<EncodedInputChunk> {
        let mut off = max_off;
        if !self.reconciled {
            match self.shared.reference_off.get() {
                None => {
                    _ = self.shared.reference_off.set(off);
                }
                Some(&reference) if off.abs_diff(reference) <= EPOCH_RECONCILE_EPSILON => {
                    off = reference;
                }
                Some(_) => {}
            }
            self.reconciled = true;
        }
        self.locked_off = Some(off);
        self.held
            .drain(..)
            .map(|mut chunk| {
                chunk.pts = off.normalize(chunk.pts);
                chunk
            })
            .collect()
    }

    /// Force-lock at the current running max and drain held frames (EOS path).
    /// Guarantees a sub-warmup clip still renders. Returns empty if already locked
    /// (held is empty) or if no frame was ever received.
    pub fn flush(&mut self) -> Vec<EncodedInputChunk> {
        if self.locked_off.is_some() {
            return Vec::new();
        }
        match self.max_off {
            Some(max_off) => self.lock_and_flush(max_off),
            None => Vec::new(),
        }
    }

    /// The offset locked for the current epoch, if any (for diagnostics/tests).
    #[cfg(test)]
    fn locked_off(&self) -> Option<EpochOffset> {
        self.locked_off
    }

    /// Mid-stream epoch discontinuity reset (moq-kit's `reset()`). Clears the lock
    /// and warmup state so the estimator re-warms and re-locks against the same,
    /// never-reset shared anchor, absorbing the input jump. `held` is empty while
    /// locked, and `reconciled` stays `true` so the re-lock keeps its own offset.
    pub fn reset(&mut self) {
        self.locked_off = None;
        self.max_off = None;
        self.plateau_frames = 0;
        self.started_elapsed = None;
    }
}

/// Detects a mid-stream epoch discontinuity: a keyframe whose raw PTS jumps more
/// than [`MOQ_EPOCH_DISCONTINUITY`] from the previous frame (moq-kit's
/// `discontinuityGapUs`). Non-keyframes and the very first frame never trigger.
pub(super) fn is_epoch_discontinuity(
    keyframe: bool,
    raw_pts: Duration,
    last_raw_pts: Option<Duration>,
) -> bool {
    keyframe && last_raw_pts.is_some_and(|last| raw_pts.abs_diff(last) > MOQ_EPOCH_DISCONTINUITY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    /// A dummy video chunk carrying its raw PTS in `pts` (as the estimator expects).
    fn chunk(raw: Duration) -> EncodedInputChunk {
        EncodedInputChunk {
            data: Bytes::new(),
            pts: raw,
            dts: None,
            kind: MediaKind::Video(VideoCodec::H264),
            present: true,
        }
    }

    fn estimator() -> LiveEdgeEstimator {
        LiveEdgeEstimator::new(EpochShared::new())
    }

    /// Feed `(raw, elapsed)` pairs and collect all emitted normalized PTS values.
    fn feed(est: &mut LiveEdgeEstimator, frames: &[(u64, u64)]) -> Vec<Duration> {
        let mut out = Vec::new();
        for &(raw, elapsed) in frames {
            for c in est.on_chunk_at(ms(elapsed), chunk(ms(raw))) {
                out.push(c.pts);
            }
        }
        out
    }

    fn assert_monotonic(pts: &[Duration]) {
        for w in pts.windows(2) {
            assert!(
                w[0] <= w[1],
                "non-monotonic output: {:?} then {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn epoch_offset_ordering_and_arithmetic() {
        let pos = EpochOffset::new(ms(100), ms(30)); // +70
        let zero = EpochOffset::new(ms(30), ms(30)); // 0
        let neg = EpochOffset::new(ms(10), ms(30)); // -20

        assert!(pos > zero && zero > neg && pos > neg);
        assert_eq!(pos.max(neg), pos);

        assert_eq!(pos.normalize(ms(100)), ms(30)); // 100 - 70
        assert_eq!(neg.normalize(ms(10)), ms(30)); // 10 - (-20)
        // saturating: normalizing below zero clamps
        assert_eq!(pos.normalize(ms(0)), ms(0));

        assert_eq!(pos.abs_diff(zero), ms(70));
        assert_eq!(pos.abs_diff(neg), ms(90)); // 70 - (-20)
        assert_eq!(neg.abs_diff(zero), ms(20));
    }

    #[test]
    fn steady_stream_locks_and_normalizes_to_zero() {
        // No-burst live start: a large-epoch track streamed at real time locks
        // within a few frames (well before the warmup deadline) at ~zero output.
        let mut est = estimator();
        let out = feed(
            &mut est,
            &[
                (1000, 0),
                (1020, 20),
                (1040, 40),
                (1060, 60), // 4th frame => plateau lock
                (1080, 80),
            ],
        );
        // Locked by the 4th frame (elapsed 60ms << 1s warmup).
        assert!(est.locked_off().is_some());
        assert_monotonic(&out);
        // First emitted normalizes to ~0 (offset absorbed the 1000ms epoch).
        assert_eq!(out[0], ms(0));
        assert_eq!(*out.last().unwrap(), ms(80));
    }

    #[test]
    fn burst_drain_locks_at_live_edge() {
        // Startup burst (raw races ahead of elapsed) then steady => lock at the
        // max once it plateaus at the live edge (~490ms).
        let mut est = estimator();
        let out = feed(
            &mut est,
            &[
                (0, 0),
                (100, 2),
                (200, 4),
                (300, 6),
                (400, 8),
                (500, 10), // caught up: off ~490
                (520, 30), // steady => plateau 1
                (540, 50), // plateau 2
                (560, 70), // plateau 3 => lock
            ],
        );
        let locked = est.locked_off().unwrap();
        assert_eq!(locked, EpochOffset::new(ms(500), ms(10))); // +490
        assert_monotonic(&out);
        assert_eq!(*out.last().unwrap(), ms(70)); // 560 - 490
    }

    #[test]
    fn eos_flush_renders_sub_warmup_clip() {
        // Too few frames to plateau-lock; EOS force-lock-and-flush emits all held.
        let mut est = estimator();
        assert!(est.on_chunk_at(ms(0), chunk(ms(100))).is_empty());
        assert!(est.on_chunk_at(ms(20), chunk(ms(120))).is_empty());
        let flushed: Vec<Duration> = est.flush().into_iter().map(|c| c.pts).collect();
        assert_eq!(flushed, vec![ms(0), ms(20)]); // offset 100 absorbed
        assert_monotonic(&flushed);
        // Flushing again after lock yields nothing.
        assert!(est.flush().is_empty());
    }

    #[test]
    fn flush_with_no_frames_is_empty() {
        let mut est = estimator();
        assert!(est.flush().is_empty());
    }

    #[test]
    fn cross_epoch_alignment_preserves_relative_offset() {
        // Audio ~0 epoch (first frame at t0); video large epoch, first frame at
        // t0 + 300ms (startup delay). Against one shared anchor the normalized
        // streams keep a ~300ms relative offset, not ~0 and not the raw ~100s gap.
        let shared = EpochShared::new();
        let mut audio = LiveEdgeEstimator::new(shared.clone());
        let mut video = LiveEdgeEstimator::new(shared);

        // Audio locks first => sets the reconciliation reference.
        let a = feed(
            &mut audio,
            &[(0, 0), (20, 20), (40, 40), (60, 60), (80, 80)],
        );
        // Video arrives 300ms later on a 100s epoch.
        let v = feed(
            &mut video,
            &[
                (100_000, 300),
                (100_033, 333),
                (100_066, 366),
                (100_099, 399), // lock
                (100_132, 432),
            ],
        );

        assert_eq!(a[0], ms(0));
        assert_eq!(v[0], ms(300));
        let rel = v[0].abs_diff(a[0]);
        assert!(
            rel.abs_diff(ms(300)) <= ms(10),
            "relative A/V offset {rel:?} should be ~300ms"
        );
    }

    #[test]
    fn reconciliation_same_epoch_adopts_reference() {
        // Both tracks share an epoch; the second (arriving with a small transport
        // delay) adopts the reference => exactly aligned output.
        let shared = EpochShared::new();
        let mut a = LiveEdgeEstimator::new(shared.clone());
        let mut b = LiveEdgeEstimator::new(shared);

        feed(&mut a, &[(0, 0), (20, 20), (40, 40), (60, 60)]);
        let ref_off = a.locked_off().unwrap();

        // B: same epoch, but observed 10ms late each frame (off = -10ms).
        let out_b = feed(&mut b, &[(0, 10), (20, 30), (40, 50), (60, 70)]);
        // Within 50ms epsilon => adopts reference exactly.
        assert_eq!(b.locked_off().unwrap(), ref_off);
        // Reference is offset 0 => normalized == raw.
        assert_eq!(out_b[0], ms(0));
    }

    #[test]
    fn reconciliation_distant_epoch_keeps_own() {
        // Second track's epoch is seconds away => keeps its own offset.
        let shared = EpochShared::new();
        let mut a = LiveEdgeEstimator::new(shared.clone());
        let mut b = LiveEdgeEstimator::new(shared);

        feed(&mut a, &[(0, 0), (20, 20), (40, 40), (60, 60)]);

        // B on a 5s epoch.
        let out_b = feed(&mut b, &[(5000, 0), (5020, 20), (5040, 40), (5060, 60)]);
        assert_eq!(b.locked_off().unwrap(), EpochOffset::new(ms(5000), ms(0)));
        assert_eq!(out_b[0], ms(0)); // 5000 - 5000, no false collapse to raw
    }

    #[test]
    fn discontinuity_resets_and_stays_continuous() {
        // Lock on epoch A, stream, then a keyframe jump to epoch B resets the
        // estimator; re-locking against the never-reset anchor keeps output
        // continuous (tracks wall-clock) instead of jumping.
        let mut est = estimator();
        let mut all = feed(&mut est, &[(1000, 0), (1020, 20), (1040, 40), (1060, 60)]);
        all.extend(feed(&mut est, &[(1080, 80), (1100, 100)]));
        let before = *all.last().unwrap();

        // Discontinuity detected upstream => reset.
        assert!(is_epoch_discontinuity(true, ms(50_000), Some(ms(1100))));
        est.reset();

        let after = feed(
            &mut est,
            &[(50_000, 120), (50_020, 140), (50_040, 160), (50_060, 180)],
        );
        all.extend(after.iter().copied());
        assert_monotonic(&all);
        assert!(
            after[0] >= before,
            "output jumped backwards on discontinuity"
        );
        // Re-locked offset absorbs the 50s jump, tracking wall-clock elapsed.
        assert_eq!(after[0], ms(120));
    }

    #[test]
    fn discontinuity_detection_conditions() {
        // Non-keyframe never resets, even on a huge jump.
        assert!(!is_epoch_discontinuity(false, ms(50_000), Some(ms(0))));
        // First frame (no previous) never resets.
        assert!(!is_epoch_discontinuity(true, ms(50_000), None));
        // Small jump under threshold does not reset.
        assert!(!is_epoch_discontinuity(true, ms(400), Some(ms(0))));
        // Keyframe + large jump resets.
        assert!(is_epoch_discontinuity(true, ms(600), Some(ms(0))));
    }
}
