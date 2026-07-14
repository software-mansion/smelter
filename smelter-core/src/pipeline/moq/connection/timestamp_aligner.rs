use std::{
    cmp::Ordering,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use tracing::debug;

use crate::prelude::*;

/// Measured A/V skew (offset between the audio and video PTS epochs) at or below
/// this is treated as a single-epoch publisher: both tracks
/// anchor to the first timestamp received on either track and skip live-edge
/// estimation. Above it we fall back to per-track live-edge locking.
const AV_SKEW_MAX: Duration = Duration::from_secs(2);
/// Fallback lock deadline for streams that trickle in without a startup burst
/// (publisher just went live, sparse/low-fps tracks).
const MOQ_EPOCH_MAX_WARMUP: Duration = Duration::from_secs(1);
/// Consecutive frames that fail to raise the running max by more than
/// [`PLATEAU_EPSILON`] before we consider the startup burst drained (live edge
/// reached) and lock.
const PLATEAU_FRAMES: u32 = 3;
/// Tolerance for "the running max did not rise" when counting plateau frames.
const PLATEAU_EPSILON: Duration = Duration::from_millis(5);
/// Minimum keyframe-to-keyframe raw-PTS forward step to even consider an epoch
/// change. Below this the step is normal group cadence, not a discontinuity.
const MOQ_EPOCH_MIN_STEP: Duration = Duration::from_millis(100);
/// Above a [`MOQ_EPOCH_MIN_STEP`] forward jump, a per-frame offset (`raw −
/// elapsed`) shift larger than this marks a mid-stream epoch change (raw advanced
/// faster than wall-clock). Offset shifts within A/V-skew tolerance are normal
/// cross-track wobble, so this equals [`AV_SKEW_MAX`] — same "how far apart before
/// it's a different epoch" scale — while staying independently tunable.
const MOQ_EPOCH_OFFSET_JUMP: Duration = Duration::from_secs(2);

/// Signed offset `raw_pts − elapsed` (a track's raw PTS at the shared anchor
/// instant). Negative when a track's near-zero raw PTS is first observed
/// well *after* another track set the anchor.
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

    fn normalize(self, raw: Duration) -> Duration {
        if self.negative {
            raw + self.magnitude
        } else {
            raw.saturating_sub(self.magnitude)
        }
    }

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TrackKind {
    Audio,
    Video,
}

impl TrackKind {
    fn other(self) -> Self {
        match self {
            TrackKind::Audio => TrackKind::Video,
            TrackKind::Video => TrackKind::Audio,
        }
    }
}

/// Shared across both track tasks: the wall-clock anchor (set once on the first
/// frame from ANY track), the small-skew anchor offset (the offset of that same
/// first frame, subtracted by both tracks so their relative A/V offset is
/// preserved), and the per-track first offsets used to measure A/V skew.
#[derive(Clone)]
pub(super) struct EpochShared {
    anchor: Arc<OnceLock<Instant>>,
    anchor_offset: Arc<OnceLock<EpochOffset>>,
    first_offset_audio: Arc<OnceLock<EpochOffset>>,
    first_offset_video: Arc<OnceLock<EpochOffset>>,
}

impl EpochShared {
    pub fn new() -> Self {
        Self {
            anchor: Arc::new(OnceLock::new()),
            anchor_offset: Arc::new(OnceLock::new()),
            first_offset_audio: Arc::new(OnceLock::new()),
            first_offset_video: Arc::new(OnceLock::new()),
        }
    }

    /// Elapsed since the shared anchor, initializing the anchor on the first call
    /// from any track.
    pub fn elapsed(&self) -> Duration {
        self.anchor.get_or_init(Instant::now).elapsed()
    }

    fn first_offset(&self, kind: TrackKind) -> Option<EpochOffset> {
        match kind {
            TrackKind::Audio => self.first_offset_audio.get().copied(),
            TrackKind::Video => self.first_offset_video.get().copied(),
        }
    }

    /// The shared small-skew anchor offset, if the first frame has been seen.
    fn anchor_offset(&self) -> Option<EpochOffset> {
        self.anchor_offset.get().copied()
    }

    /// Record a track's first observed offset (set-once).
    fn set_first_track_offset(&self, kind: TrackKind, offset: EpochOffset) {
        match kind {
            TrackKind::Audio => _ = self.first_offset_audio.set(offset),
            TrackKind::Video => _ = self.first_offset_video.set(offset),
        }
    }

    /// Try to claim the shared small-skew anchor with `offset` (set-once, so only the
    /// genuinely first frame across both tracks wins).
    fn set_anchor_offset(&self, offset: EpochOffset) {
        _ = self.anchor_offset.set(offset);
    }
}

/// Outcome of the first-epoch skew branch, disambiguating "large skew confirmed"
/// (`LiveEdge`) from "counterpart not seen yet" (`Pending`).
enum SkewDecision {
    /// Anchor both tracks to the shared first timestamp (single track or small skew).
    Anchor(EpochOffset),
    /// Skew exceeds [`AV_SKEW_MAX`]: lock this track at its own live edge.
    LiveEdge,
    /// Counterpart's first frame not observed yet; skew not measurable.
    Pending,
}

/// Per-track, loop-local. Normalizes the track's PTS epoch to the shared anchor.
///
/// In the common single-epoch case (single track, or A/V skew <= [`AV_SKEW_MAX`])
/// it locks immediately to the first timestamp received on either track — no
/// warmup, relative A/V offset preserved by construction. Only when the skew
/// exceeds [`AV_SKEW_MAX`] (or after a mid-stream discontinuity `reset`) does it
/// fall back to live-edge estimation: the running max of `raw − elapsed`, locked
/// once the max plateaus (startup burst drained) or a fallback deadline fires.
/// Frames are held until lock (~ms of real wall-clock, just the burst window) so
/// the locked constant applies from the first *emitted* frame and output is
/// monotonic by construction.
///
/// Latency-skew assumption: edge sync aligns each track's newest-available sample
/// to "now", so materially different per-track transport latency leaves a fixed
/// residual A/V skew. This is inherent to edge-based sync;
/// there is no on-wire capture-time signal to do better without a publisher change.
pub(super) struct TimestampAligner {
    shared: EpochShared,
    kind: TrackKind,
    single_track_stream: bool,
    /// Time elapsed from the shared anchor at the first observed frame of each epoch;
    /// `None` until then.
    epoch_start_elapsed: Option<Duration>,
    /// Running max of `raw − elapsed`; equals the live-edge offset.
    max_offset: Option<EpochOffset>,
    /// Frames buffered until lock; each carries its raw PTS in `chunk.pts`.
    held: Vec<EncodedInputChunk>,
    locked_offset: Option<EpochOffset>,
    /// Previous frame's `(raw_pts, offset)`, updated on every frame (locked or
    /// warming). Baseline for the epoch-discontinuity check. `reset()` does *not*
    /// clear it: the post-jump frame becomes the next baseline.
    previous: Option<(Duration, EpochOffset)>,

    /// Consecutive frames that did not raise the max by more than [`PLATEAU_EPSILON`].
    plateau_frames: u32,
    /// True until the first lock; `reset()` leaves it `false`.
    first_epoch: bool,
    skew_decided: bool,
}

impl TimestampAligner {
    pub fn new(shared: EpochShared, kind: TrackKind, single_track_stream: bool) -> Self {
        Self {
            shared,
            kind,
            single_track_stream,
            epoch_start_elapsed: None,
            max_offset: None,
            plateau_frames: 0,
            held: Vec::new(),
            locked_offset: None,
            first_epoch: true,
            skew_decided: false,
            previous: None,
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked_offset.is_some()
    }

    /// Clears the lock and warmup state so the aligner re-warms and re-locks
    /// against the same, never-reset shared anchor.
    pub fn reset(&mut self) {
        self.locked_offset = None;
        self.max_offset = None;
        self.plateau_frames = 0;
        self.epoch_start_elapsed = None;
        self.held = Vec::new();
        self.first_epoch = false;
    }

    /// Feed one frame (with its raw PTS in `chunk.pts`). Detects a mid-stream
    /// epoch discontinuity against the previous frame and resets the aligner
    /// before normalizing. Returns the chunks ready to emit: empty while warming
    /// (chunk held), the full flushed batch at lock, or the single normalized
    /// chunk once locked.
    pub fn on_chunk(
        &mut self,
        keyframe: bool,
        mut chunk: EncodedInputChunk,
    ) -> Vec<EncodedInputChunk> {
        let elapsed = self.shared.elapsed();
        let raw = chunk.pts;
        let offset = EpochOffset::new(raw, elapsed);
        if is_epoch_discontinuity(keyframe, raw, offset, self.previous) {
            debug!(?raw, "MoQ epoch discontinuity detected, resetting aligner.");
            self.reset();
        }
        self.previous = Some((raw, offset));

        match self.locked_offset {
            Some(offset) => {
                chunk.pts = offset.normalize(raw);
                vec![chunk]
            }
            None => self.advance_warmup(raw, elapsed, chunk),
        }
    }

    fn advance_warmup(
        &mut self,
        raw: Duration,
        elapsed: Duration,
        chunk: EncodedInputChunk,
    ) -> Vec<EncodedInputChunk> {
        let offset = EpochOffset::new(raw, elapsed);

        // On the first frame of the first epoch: record this track's first offset and try
        // to claim the shared anchor (OnceLock => only the genuinely first frame
        // across both tracks wins).
        if self.first_epoch && self.epoch_start_elapsed.is_none() {
            self.shared.set_first_track_offset(self.kind, offset);
            self.shared.set_anchor_offset(offset);
        }

        // Keep accumulating the running max / plateau counter (still needed for the
        // >2s and post-reset live-edge paths). In case of live streaming, frames only ever arrive late,
        // so `offset <= edge` and the max climbs from below with no overshoot; it
        // plateaus once the burst drains.
        let prev = self.max_offset;
        let max_offset = prev.map_or(offset, |p| p.max(offset));
        self.max_offset = Some(max_offset);
        match prev.is_some_and(|p| max_offset.abs_diff(p) <= PLATEAU_EPSILON) {
            true => self.plateau_frames += 1,
            false => self.plateau_frames = 0,
        }
        self.held.push(chunk);

        let started = *self.epoch_start_elapsed.get_or_insert(elapsed);

        // Anchor to the shared first timestamp unless the A/V skew is confirmed to exceed AV_SKEW_MAX.
        // Decision is made only once on the first frame of the first epoch.
        if self.first_epoch && !self.skew_decided {
            match self.skew_decision() {
                SkewDecision::Anchor(anchor) => {
                    self.skew_decided = true;
                    self.lock_and_flush(anchor)
                }
                SkewDecision::LiveEdge => {
                    self.skew_decided = true;
                    match self.live_edge_settled(elapsed, started) {
                        true => self.lock_and_flush(max_offset),
                        false => Vec::new(),
                    }
                }
                SkewDecision::Pending if self.warmup_deadline_passed(elapsed, started) => {
                    // Counterpart's first frame not seen yet: keep buffering, but
                    // still honor the warmup deadline as a live-edge fallback.
                    self.skew_decided = true;
                    self.lock_and_flush(max_offset)
                }
                _ => Vec::new(),
            }
        } else if self.live_edge_settled(elapsed, started) {
            // After the first epoch (large-skew fallback or post-reset): live-edge lock.
            self.lock_and_flush(max_offset)
        } else {
            Vec::new()
        }
    }

    fn lock_and_flush(&mut self, offset: EpochOffset) -> Vec<EncodedInputChunk> {
        self.locked_offset = Some(offset);
        self.first_epoch = false;
        self.held
            .drain(..)
            .map(|mut chunk| {
                chunk.pts = offset.normalize(chunk.pts);
                chunk
            })
            .collect()
    }

    /// Force-lock at the current running max and drain the frames held during
    /// warmup (EOS path), so a sub-warmup clip still renders. Caller must ensure
    /// the aligner is still warming up (`!is_locked()`); returns empty if no
    /// frame was ever received.
    pub fn flush(&mut self) -> Vec<EncodedInputChunk> {
        debug_assert!(!self.is_locked());
        match self.max_offset {
            Some(max_offset) => self.lock_and_flush(max_offset),
            None => Vec::new(),
        }
    }

    fn skew_decision(&self) -> SkewDecision {
        let anchor = self
            .shared
            .anchor_offset()
            .expect("anchor offset set on the first frame");

        // Single track: no counterpart => trivially small skew.
        if self.single_track_stream {
            return SkewDecision::Anchor(anchor);
        }

        let Some(other_first) = self.shared.first_offset(self.kind.other()) else {
            return SkewDecision::Pending;
        };
        let own_first = self
            .shared
            .first_offset(self.kind)
            .expect("own first offset published on the first frame");

        if own_first.abs_diff(other_first) <= AV_SKEW_MAX {
            SkewDecision::Anchor(anchor)
        } else {
            SkewDecision::LiveEdge
        }
    }

    fn live_edge_settled(&self, elapsed: Duration, started: Duration) -> bool {
        self.plateau_frames >= PLATEAU_FRAMES || self.warmup_deadline_passed(elapsed, started)
    }

    fn warmup_deadline_passed(&self, elapsed: Duration, started: Duration) -> bool {
        elapsed.saturating_sub(started) > MOQ_EPOCH_MAX_WARMUP
    }
}

fn is_epoch_discontinuity(
    keyframe: bool,
    raw_pts: Duration,
    offset: EpochOffset,
    previous: Option<(Duration, EpochOffset)>, // (previous_raw_pts, previous_offset)
) -> bool {
    if !keyframe {
        return false;
    }
    let Some((previous_raw_pts, previous_offset)) = previous else {
        return false;
    };
    // 1. Small forward step -> normal group cadence, not an epoch change.
    if raw_pts >= previous_raw_pts && raw_pts - previous_raw_pts < MOQ_EPOCH_MIN_STEP {
        return false;
    }
    // 2. Time went backwards -> clock reset / new epoch.
    if raw_pts < previous_raw_pts {
        return true;
    }
    // 3. Forward jump >= MOQ_EPOCH_MIN_STEP -> disambiguate by offset delta: a real
    //    epoch change steps the offset (raw outran wall-clock), a same-epoch drop
    //    leaves it ~unchanged.
    offset.abs_diff(previous_offset) > MOQ_EPOCH_OFFSET_JUMP
}
