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

    /// `|self − other|`, for the skew / plateau epsilon checks.
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

/// Which of the two tracks an aligner handles. Used to index the per-track
/// first-offset slots and to look up the counterpart's first offset for the skew
/// decision.
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
    /// Which track this aligner handles.
    kind: TrackKind,
    /// Whether this is the only track (no counterpart; audio or video absent).
    /// When true the track is trivially single-epoch and anchors to its own first
    /// timestamp on the first frame.
    single_track_stream: bool,
    /// Shared elapsed at the first observed frame (warmup start); `None` until then.
    started_elapsed: Option<Duration>,
    /// Running max of `raw − elapsed`; equals the live-edge offset.
    max_offset: Option<EpochOffset>,
    /// Consecutive frames that did not raise the max by more than [`PLATEAU_EPSILON`].
    plateau_frames: u32,
    /// Frames buffered until lock; each carries its raw PTS in `chunk.pts`.
    held: Vec<EncodedInputChunk>,
    locked_offset: Option<EpochOffset>,
    /// True until the first lock; the first-timestamp anchor path only applies
    /// while this holds. `reset()` leaves it `false`, so post-discontinuity
    /// re-locks always go through live-edge estimation (the anchor is stale after
    /// an epoch jump).
    first_epoch: bool,
    /// Set once the skew is measured to exceed [`AV_SKEW_MAX`]: from then on the
    /// first epoch locks via live-edge, not the anchor.
    decided_live_edge: bool,
    /// Previous frame's `(raw_pts, offset)`, updated on every frame (locked or
    /// warming). Baseline for the epoch-discontinuity check. `reset()` does *not*
    /// clear it: the post-jump frame becomes the next baseline.
    previous: Option<(Duration, EpochOffset)>,
}

impl TimestampAligner {
    pub fn new(shared: EpochShared, kind: TrackKind, single_track_stream: bool) -> Self {
        Self {
            shared,
            kind,
            single_track_stream,
            started_elapsed: None,
            max_offset: None,
            plateau_frames: 0,
            held: Vec::new(),
            locked_offset: None,
            first_epoch: true,
            decided_live_edge: false,
            previous: None,
        }
    }

    /// Feed one frame (with its raw PTS in `chunk.pts`). Detects a mid-stream
    /// epoch discontinuity against the previous frame and resets the aligner
    /// before normalizing. Returns the chunks ready to emit: empty while warming
    /// (chunk held), the full flushed batch at lock, or the single normalized
    /// chunk once locked.
    pub fn on_frame(
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

        // First frame of the first epoch: record this track's first offset and try
        // to claim the shared anchor (OnceLock => only the genuinely first frame
        // across both tracks wins).
        if self.first_epoch && self.started_elapsed.is_none() {
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
        if prev.is_some_and(|p| max_offset.abs_diff(p) <= PLATEAU_EPSILON) {
            self.plateau_frames += 1;
        } else {
            self.plateau_frames = 0;
        }
        self.held.push(chunk);

        let started = *self.started_elapsed.get_or_insert(elapsed);

        // First-epoch small-skew decision: anchor to the shared first timestamp
        // unless the A/V skew is confirmed to exceed AV_SKEW_MAX.
        if self.first_epoch && !self.decided_live_edge {
            match self.skew_decision() {
                SkewDecision::Anchor(anchor) => self.lock_and_flush(anchor),
                SkewDecision::LiveEdge => {
                    // Large skew confirmed: fall through to the live-edge lock.
                    self.decided_live_edge = true;
                    self.maybe_live_edge_lock(max_offset, elapsed, started)
                }
                SkewDecision::Pending => {
                    // Counterpart's first frame not seen yet: keep buffering, but
                    // still honor the warmup deadline as a live-edge fallback.
                    if elapsed.saturating_sub(started) >= MOQ_EPOCH_MAX_WARMUP {
                        self.lock_and_flush(max_offset)
                    } else {
                        Vec::new()
                    }
                }
            }
        } else {
            // After the first epoch (large-skew fallback or post-reset): live-edge lock.
            self.maybe_live_edge_lock(max_offset, elapsed, started)
        }
    }

    /// Decide how to lock during the first epoch: anchor to the shared first
    /// timestamp when there is no counterpart or the A/V skew is small, fall back
    /// to live-edge when the skew is large, or defer until the counterpart's first
    /// frame makes the skew measurable.
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

    /// Live-edge lock once the burst plateaus or the warmup deadline fires.
    fn maybe_live_edge_lock(
        &mut self,
        max_offset: EpochOffset,
        elapsed: Duration,
        started: Duration,
    ) -> Vec<EncodedInputChunk> {
        if self.plateau_frames >= PLATEAU_FRAMES
            || elapsed.saturating_sub(started) >= MOQ_EPOCH_MAX_WARMUP
        {
            self.lock_and_flush(max_offset)
        } else {
            Vec::new()
        }
    }

    /// Lock at the given offset and return all held chunks normalized with it.
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

    /// True once the aligner has locked its epoch offset (warmup finished).
    /// While false it is still warming up and holding frames not yet emitted.
    pub fn is_locked(&self) -> bool {
        self.locked_offset.is_some()
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

    /// Mid-stream epoch discontinuity reset. Clears the lock
    /// and warmup state so the aligner re-warms and re-locks against the same,
    /// never-reset shared anchor, absorbing the input jump. `held` is empty while
    /// locked, and `first_epoch` stays `false` so the re-lock goes straight to
    /// live-edge (the first-timestamp anchor is stale after an epoch jump).
    pub fn reset(&mut self) {
        self.locked_offset = None;
        self.max_offset = None;
        self.plateau_frames = 0;
        self.started_elapsed = None;
        self.decided_live_edge = false;
    }
}

/// Detects a mid-stream epoch discontinuity by comparing consecutive-frame
/// offsets (`raw − elapsed`) rather than raw PTS alone, so a same-epoch content
/// gap (a stalled group dropped, delivered in real time so `Δraw ≈ Δelapsed`)
/// does not masquerade as a real epoch change (publisher restart / clock switch,
/// where `raw` jumps while wall-clock does not). Gated on keyframes (group
/// boundaries) so a real epoch change aligns its reset to a decodable boundary
/// and mid-GOP noise is filtered. Non-keyframes and the very first frame never
/// trigger.
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

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    /// A dummy video chunk carrying its raw PTS in `pts` (as the aligner expects).
    fn chunk(raw: Duration) -> EncodedInputChunk {
        EncodedInputChunk {
            data: Bytes::new(),
            pts: raw,
            dts: None,
            kind: MediaKind::Video(VideoCodec::H264),
            present: true,
        }
    }

    /// A single-track aligner (no counterpart): anchors to its own first
    /// timestamp on the first frame.
    fn aligner() -> TimestampAligner {
        TimestampAligner::new(EpochShared::new(), TrackKind::Video, true)
    }

    /// A two-track aligner forced onto the live-edge path: its counterpart sits
    /// ~5s away, so the measured A/V skew exceeds [`AV_SKEW_MAX`].
    fn live_edge_aligner() -> TimestampAligner {
        let shared = EpochShared::new();
        shared.set_first_track_offset(TrackKind::Audio, EpochOffset::new(ms(5000), ms(0)));
        TimestampAligner::new(shared, TrackKind::Video, false)
    }

    /// Feed `(raw, elapsed)` pairs and collect all emitted normalized PTS values.
    ///
    /// Mirrors `on_frame`'s dispatch with a test-controlled `elapsed` (instead of
    /// the real wall clock): once the aligner has locked, frames take the
    /// normalize path; while warming they go through `advance_warmup`.
    fn feed(aligner: &mut TimestampAligner, frames: &[(u64, u64)]) -> Vec<Duration> {
        let mut out = Vec::new();
        for &(raw, elapsed) in frames {
            let emitted = match aligner.locked_offset {
                Some(offset) => {
                    let mut c = chunk(ms(raw));
                    c.pts = offset.normalize(ms(raw));
                    vec![c]
                }
                None => aligner.advance_warmup(ms(raw), ms(elapsed), chunk(ms(raw))),
            };
            for c in emitted {
                out.push(c.pts);
            }
        }
        out
    }

    /// Shorthand for an `EpochOffset` built from `(raw, elapsed)` in ms.
    fn off(raw: u64, elapsed: u64) -> EpochOffset {
        EpochOffset::new(ms(raw), ms(elapsed))
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
        // No-burst live start on the live-edge path: a large-epoch track streamed
        // at real time locks within a few frames (well before the warmup deadline)
        // at ~zero output.
        let mut aligner = live_edge_aligner();
        let out = feed(
            &mut aligner,
            &[
                (1000, 0),
                (1020, 20),
                (1040, 40),
                (1060, 60), // 4th frame => plateau lock
                (1080, 80),
            ],
        );
        // Locked by the 4th frame (elapsed 60ms << 1s warmup).
        assert!(aligner.locked_offset.is_some());
        assert_monotonic(&out);
        // First emitted normalizes to ~0 (offset absorbed the 1000ms epoch).
        assert_eq!(out[0], ms(0));
        assert_eq!(*out.last().unwrap(), ms(80));
    }

    #[test]
    fn burst_drain_locks_at_live_edge() {
        // Startup burst (raw races ahead of elapsed) then steady => lock at the
        // max once it plateaus at the live edge (~490ms).
        let mut aligner = live_edge_aligner();
        let out = feed(
            &mut aligner,
            &[
                (0, 0),
                (100, 2),
                (200, 4),
                (300, 6),
                (400, 8),
                (500, 10), // caught up: offset ~490
                (520, 30), // steady => plateau 1
                (540, 50), // plateau 2
                (560, 70), // plateau 3 => lock
            ],
        );
        let locked = aligner.locked_offset.unwrap();
        assert_eq!(locked, EpochOffset::new(ms(500), ms(10))); // +490
        assert_monotonic(&out);
        assert_eq!(*out.last().unwrap(), ms(70)); // 560 - 490
    }

    #[test]
    fn eos_flush_renders_sub_warmup_clip() {
        // Too few frames to plateau-lock; EOS force-lock-and-flush emits all held.
        let mut aligner = live_edge_aligner();
        assert!(
            aligner
                .advance_warmup(ms(100), ms(0), chunk(ms(100)))
                .is_empty()
        );
        assert!(
            aligner
                .advance_warmup(ms(120), ms(20), chunk(ms(120)))
                .is_empty()
        );
        let flushed: Vec<Duration> = aligner.flush().into_iter().map(|c| c.pts).collect();
        assert_eq!(flushed, vec![ms(0), ms(20)]); // offset 100 absorbed
        assert_monotonic(&flushed);
        // The flush locked the aligner, so the caller won't flush again.
        assert!(aligner.is_locked());
    }

    #[test]
    fn flush_with_no_frames_is_empty() {
        let mut aligner = aligner();
        assert!(aligner.flush().is_empty());
    }

    #[test]
    fn small_skew_both_anchor_to_shared_timestamp() {
        // Single-epoch publisher (`moq-cli`-style): audio and video share a PTS
        // epoch, video arriving with a small transport delay. With skew <=
        // AV_SKEW_MAX both tracks anchor to the first timestamp seen on either
        // track, so equal raw PTS map to equal normalized output.
        let shared = EpochShared::new();
        let mut audio = TimestampAligner::new(shared.clone(), TrackKind::Audio, false);
        let mut video = TimestampAligner::new(shared, TrackKind::Video, false);

        // Audio's first frame sets the shared anchor; it holds (skew not yet
        // measurable) until video's first frame arrives.
        assert!(feed(&mut audio, &[(0, 0)]).is_empty());
        // Video: same epoch, observed 30ms late => skew 30ms, anchors immediately.
        let v = feed(&mut video, &[(0, 30), (20, 50), (40, 70)]);
        // Audio's next frame now measures the small skew and anchors too.
        let a = feed(&mut audio, &[(20, 20), (40, 40)]);

        assert_eq!(a[0], ms(0));
        assert_eq!(v[0], ms(0));
        // Same anchor for both => equal raw PTS produce equal output (exact align).
        assert_eq!(audio.locked_offset.unwrap(), video.locked_offset.unwrap());
        assert_eq!(a, v);
    }

    #[test]
    fn large_skew_each_locks_own_live_edge() {
        // Skew beyond AV_SKEW_MAX: neither track adopts the shared anchor; each
        // locks at its own live edge, so a distant epoch does not falsely collapse.
        let shared = EpochShared::new();
        let mut a = TimestampAligner::new(shared.clone(), TrackKind::Audio, false);
        let mut b = TimestampAligner::new(shared, TrackKind::Video, false);

        // Audio epoch 0, video epoch 5s (skew 5s > 2s). First frames establish
        // both epochs, then steady frames plateau-lock each at its own edge.
        feed(&mut a, &[(0, 0)]);
        feed(&mut b, &[(5000, 0)]);
        feed(&mut a, &[(20, 20), (40, 40), (60, 60)]);
        let out_b = feed(&mut b, &[(5020, 20), (5040, 40), (5060, 60)]);

        assert_eq!(a.locked_offset.unwrap(), EpochOffset::new(ms(0), ms(0)));
        assert_eq!(b.locked_offset.unwrap(), EpochOffset::new(ms(5000), ms(0)));
        assert_eq!(out_b[0], ms(0)); // 5000 - 5000, no false collapse to raw
    }

    #[test]
    fn cross_epoch_alignment_preserves_relative_offset() {
        // Browser-style cross-epoch publisher: audio ~0 epoch, video ~100s epoch
        // (skew >> AV_SKEW_MAX) so each track live-edge locks at its own edge.
        // Video's first frame lands 300ms after audio's, and that ~300ms relative
        // offset survives against the shared anchor (not collapsed to 0, not the
        // raw ~100s gap).
        let shared = EpochShared::new();
        let mut audio = TimestampAligner::new(shared.clone(), TrackKind::Audio, false);
        let mut video = TimestampAligner::new(shared, TrackKind::Video, false);

        // First frames establish both epochs; video starts 300ms late.
        feed(&mut audio, &[(0, 0)]);
        feed(&mut video, &[(100_000, 300)]);
        let a = feed(&mut audio, &[(20, 20), (40, 40), (60, 60)]);
        let v = feed(
            &mut video,
            &[(100_033, 333), (100_066, 366), (100_099, 399)],
        );

        assert_eq!(a[0], ms(0));
        let rel = v[0].abs_diff(a[0]);
        assert!(
            rel.abs_diff(ms(300)) <= ms(10),
            "relative A/V offset {rel:?} should be ~300ms"
        );
    }

    #[test]
    fn skew_boundary_at_av_skew_max() {
        // Exactly AV_SKEW_MAX still counts as small skew => anchor. Just beyond it
        // falls back to the live edge.
        {
            let shared = EpochShared::new();
            let mut audio = TimestampAligner::new(shared.clone(), TrackKind::Audio, false);
            let mut video = TimestampAligner::new(shared, TrackKind::Video, false);
            feed(&mut audio, &[(0, 0)]);
            // skew == 2000ms => anchors to the shared anchor (audio's first offset).
            feed(&mut video, &[(2000, 0)]);
            assert_eq!(video.locked_offset.unwrap(), EpochOffset::new(ms(0), ms(0)));
        }
        {
            let shared = EpochShared::new();
            let mut audio = TimestampAligner::new(shared.clone(), TrackKind::Audio, false);
            let mut video = TimestampAligner::new(shared, TrackKind::Video, false);
            feed(&mut audio, &[(0, 0)]);
            // skew 2001ms > AV_SKEW_MAX => live-edge; steady frames plateau-lock.
            let v = feed(&mut video, &[(2001, 0), (2021, 20), (2041, 40), (2061, 60)]);
            assert_eq!(
                video.locked_offset.unwrap(),
                EpochOffset::new(ms(2001), ms(0))
            );
            assert_eq!(v[0], ms(0));
        }
    }

    #[test]
    fn single_track_anchors_on_first_frame() {
        // No counterpart => trivially small skew: lock on the first frame to its
        // own first timestamp (no warmup), normalizing the first output to ~0.
        let mut aligner = aligner();
        // Large epoch, burst-y arrival; the single track ignores the burst.
        let out = feed(&mut aligner, &[(1000, 0), (1100, 5), (1200, 10)]);
        assert_eq!(
            aligner.locked_offset.unwrap(),
            EpochOffset::new(ms(1000), ms(0))
        );
        assert_monotonic(&out);
        // Locked on the very first frame => every fed frame produced output.
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], ms(0)); // first frame normalized to ~0
    }

    #[test]
    fn discontinuity_detection_conditions() {
        // Keyframe gate: a non-keyframe never resets, even on a huge raw/offset step.
        assert!(!is_epoch_discontinuity(
            false,
            ms(50_000),
            off(50_000, 0),
            Some((ms(0), off(0, 0)))
        ));
        // First frame (no previous) never resets.
        assert!(!is_epoch_discontinuity(
            true,
            ms(50_000),
            off(50_000, 0),
            None
        ));

        // Branch 1: small forward step (< MOQ_EPOCH_MIN_STEP) short-circuits before
        // the offset check, even when the offset delta would otherwise look large.
        assert!(!is_epoch_discontinuity(
            true,
            ms(1033),
            off(1033, 50_000),
            Some((ms(1000), off(1000, 0)))
        ));

        // Branch 2: time went backwards => reset, regardless of magnitude/offset.
        assert!(is_epoch_discontinuity(
            true,
            ms(1000),
            off(1000, 60_000),
            Some((ms(50_000), off(50_000, 60_000)))
        ));

        // Branch 3, small offset delta: raw +2s and elapsed +2s (same-epoch drop) =>
        // stable offset => no reset.
        assert!(!is_epoch_discontinuity(
            true,
            ms(3000),
            off(3000, 2000),
            Some((ms(1000), off(1000, 0)))
        ));

        // Branch 3, large offset delta: raw +50s, elapsed unchanged => offset steps
        // ~50s => reset.
        assert!(is_epoch_discontinuity(
            true,
            ms(51_000),
            off(51_000, 0),
            Some((ms(1000), off(1000, 0)))
        ));
    }
}
