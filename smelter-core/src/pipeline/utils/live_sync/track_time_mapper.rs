use std::time::Duration;

use super::live_edge_estimator::saturating_duration_from_nanos;

#[derive(Debug, Clone, Copy)]
pub(crate) struct TrackTimeMapperOptions {
    /// Raw PTS jump larger than this is reported as a discontinuity.
    pub discontinuity_threshold: Duration,
    /// Fraction of media-time advance spent on applying pending gradual
    /// shifts; 0.01 means content plays up to 1% faster or slower while a
    /// correction is in progress.
    pub warp_ratio: f64,
}

impl Default for TrackTimeMapperOptions {
    fn default() -> Self {
        Self {
            discontinuity_threshold: Duration::from_secs(10),
            warp_ratio: 0.01,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapOutcome {
    Pts {
        pts: Duration,
        dts: Option<Duration>,
    },
    /// Packet is older than the committed baseline (e.g. a pre-join
    /// leftover); the caller should drop it.
    BeforeBaseline,
    /// Raw PTS jumped more than `discontinuity_threshold` away from the
    /// previous packet. The mapper state is left unchanged; the caller is
    /// expected to restart the track (reset, re-probe, commit a new
    /// baseline).
    Discontinuity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShiftDirection {
    /// Content plays earlier: catches up towards the live edge, reduces
    /// latency.
    Earlier,
    /// Content plays later: adds latency, refills the safety buffer.
    Later,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShiftMode {
    /// Consumed incrementally by subsequent [`TrackTimeMapper::map`] calls,
    /// bounded by `warp_ratio`; invisible to the viewer.
    Gradual,
    /// Applied in full at once. `Earlier` assumes the caller dropped the
    /// corresponding span of content (e.g. skipped to a keyframe); the
    /// expected next raw PTS is advanced accordingly so the jump is not
    /// reported as a discontinuity.
    Immediate,
}

/// Maps raw source timestamps (container time, unit-converted to `Duration`
/// but not shifted) to track time: the zero-based timestamps sent downstream
/// in `EncodedInputChunk` and interpreted by the queue relative to the
/// track's registered offset.
///
/// `track_pts = raw_pts - baseline`, where the baseline is set explicitly by
/// [`Self::commit_baseline`] (the join decision) and later adjusted through
/// [`Self::shift`] — the single knob that moves an input against the live
/// edge. Owns what inputs currently hand-roll separately: `first_pts`
/// normalization, discontinuity detection and gradual PTS warping.
///
/// One mapper per input, shared between its audio and video threads (wrap in
/// a mutex) so both tracks stay on one baseline.
#[derive(Debug)]
pub(crate) struct TrackTimeMapper {
    options: TrackTimeMapperOptions,
    /// Raw PTS mapped to track time zero. Signed: `Later` shifts can move it
    /// below zero.
    baseline_nanos: Option<i128>,
    /// Remaining gradual shift; positive raises the baseline (content plays
    /// earlier).
    pending_shift_nanos: i128,
    /// Newest raw PTS seen; reference point for discontinuity detection and
    /// warp budget.
    last_raw_pts: Duration,
    /// Offset the queue track was registered with (`QueueTrackOffset::Pts`).
    queue_offset: Option<Duration>,
}

impl TrackTimeMapper {
    pub fn new(options: TrackTimeMapperOptions) -> Self {
        Self {
            options,
            baseline_nanos: None,
            pending_shift_nanos: 0,
            last_raw_pts: Duration::ZERO,
            queue_offset: None,
        }
    }

    /// Sets the raw PTS that maps to track time zero. Must be called before
    /// [`Self::map`]; calling again rebases the mapping and drops any
    /// pending gradual shift.
    pub fn commit_baseline(&mut self, pts: Duration) {
        self.baseline_nanos = Some(pts.as_nanos() as i128);
        self.pending_shift_nanos = 0;
        self.last_raw_pts = pts;
    }

    #[allow(dead_code)]
    pub fn is_committed(&self) -> bool {
        self.baseline_nanos.is_some()
    }

    /// Records the offset the queue track was registered with, enabling
    /// [`Self::playhead`].
    pub fn set_queue_offset(&mut self, offset: Duration) {
        self.queue_offset = Some(offset);
    }

    pub fn map(&mut self, pts: Duration, dts: Option<Duration>) -> MapOutcome {
        let Some(_) = self.baseline_nanos else {
            panic!("TrackTimeMapper::map called before commit_baseline");
        };
        if self.last_raw_pts.abs_diff(pts) > self.options.discontinuity_threshold {
            return MapOutcome::Discontinuity;
        }

        self.apply_pending_shift(pts.saturating_sub(self.last_raw_pts));
        self.last_raw_pts = Duration::max(self.last_raw_pts, pts);

        let baseline = self.baseline_nanos.unwrap();
        let track_pts = pts.as_nanos() as i128 - baseline;
        if track_pts < 0 {
            return MapOutcome::BeforeBaseline;
        }
        MapOutcome::Pts {
            pts: saturating_duration_from_nanos(track_pts),
            dts: dts.map(|dts| saturating_duration_from_nanos(dts.as_nanos() as i128 - baseline)),
        }
    }

    /// Moves the mapping by `delta`. The caller is responsible for keeping
    /// emitted track PTS monotonic: an `Immediate` `Earlier` shift must be
    /// paired with dropping at least `delta` of raw content.
    pub fn shift(&mut self, delta: Duration, direction: ShiftDirection, mode: ShiftMode) {
        let Some(baseline) = &mut self.baseline_nanos else {
            panic!("TrackTimeMapper::shift called before commit_baseline");
        };
        let delta_nanos = match direction {
            ShiftDirection::Earlier => delta.as_nanos() as i128,
            ShiftDirection::Later => -(delta.as_nanos() as i128),
        };
        match mode {
            ShiftMode::Immediate => {
                *baseline += delta_nanos;
                if direction == ShiftDirection::Earlier {
                    self.last_raw_pts += delta;
                }
            }
            ShiftMode::Gradual => self.pending_shift_nanos += delta_nanos,
        }
    }

    /// Raw PTS the queue is consuming at `queue_now` (duration since the
    /// queue sync point, e.g. `effective_last_pts()`). Feed the result to
    /// `LiveEdgeSnapshot::lag_behind`. None until the baseline is committed
    /// and the queue offset is set.
    #[allow(dead_code)]
    pub fn playhead(&self, queue_now: Duration) -> Option<Duration> {
        Some(saturating_duration_from_nanos(
            self.playhead_nanos(queue_now)?,
        ))
    }

    /// Same as [`Self::playhead`] but signed: negative until the queue
    /// reaches the track's offset (right after a join the playback position
    /// is legitimately before the baseline).
    pub fn playhead_nanos(&self, queue_now: Duration) -> Option<i128> {
        let baseline = self.baseline_nanos?;
        let offset = self.queue_offset?;
        Some(queue_now.as_nanos() as i128 - offset.as_nanos() as i128 + baseline)
    }

    /// Newest raw PTS seen by `map`; the point content resumes from after a
    /// skip.
    pub fn last_raw_pts(&self) -> Duration {
        self.last_raw_pts
    }

    /// Remaining gradual shift in nanoseconds; positive plays content
    /// earlier.
    pub fn pending_shift_nanos(&self) -> i128 {
        self.pending_shift_nanos
    }

    pub fn cancel_pending_shift(&mut self) {
        self.pending_shift_nanos = 0;
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.baseline_nanos = None;
        self.pending_shift_nanos = 0;
        self.last_raw_pts = Duration::ZERO;
        self.queue_offset = None;
    }

    fn apply_pending_shift(&mut self, media_advance: Duration) {
        if self.pending_shift_nanos == 0 {
            return;
        }
        let budget = (media_advance.as_nanos() as f64 * self.options.warp_ratio) as i128;
        let step = i128::min(budget, self.pending_shift_nanos.abs())
            * self.pending_shift_nanos.signum();
        if let Some(baseline) = &mut self.baseline_nanos {
            *baseline += step;
        }
        self.pending_shift_nanos -= step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn mapper() -> TrackTimeMapper {
        TrackTimeMapper::new(TrackTimeMapperOptions::default())
    }

    fn expect_pts(outcome: MapOutcome) -> Duration {
        match outcome {
            MapOutcome::Pts { pts, .. } => pts,
            other => panic!("expected Pts, got {other:?}"),
        }
    }

    #[test]
    fn basic_mapping() {
        let mut mapper = mapper();
        mapper.commit_baseline(ms(5000));
        assert_eq!(
            mapper.map(ms(5000), Some(ms(4900))),
            MapOutcome::Pts {
                pts: ms(0),
                // dts below baseline saturates to zero
                dts: Some(ms(0)),
            }
        );
        assert_eq!(
            mapper.map(ms(5100), Some(ms(5033))),
            MapOutcome::Pts {
                pts: ms(100),
                dts: Some(ms(33)),
            }
        );
    }

    #[test]
    fn drops_packets_before_baseline() {
        let mut mapper = mapper();
        mapper.commit_baseline(ms(5000));
        assert_eq!(mapper.map(ms(4500), None), MapOutcome::BeforeBaseline);
        // stream continues normally afterwards
        assert_eq!(expect_pts(mapper.map(ms(5033), None)), ms(33));
    }

    #[test]
    #[should_panic]
    fn map_before_commit_panics() {
        let mut mapper = mapper();
        mapper.map(ms(0), None);
    }

    #[test]
    fn detects_discontinuity() {
        let mut mapper = mapper();
        mapper.commit_baseline(ms(60_000));
        assert_eq!(expect_pts(mapper.map(ms(60_100), None)), ms(100));
        // forward jump
        assert_eq!(mapper.map(ms(80_000), None), MapOutcome::Discontinuity);
        // backward jump (e.g. source restarted from zero)
        assert_eq!(mapper.map(ms(0), None), MapOutcome::Discontinuity);
        // state is unchanged, in-range packets still map
        assert_eq!(expect_pts(mapper.map(ms(60_200), None)), ms(200));
    }

    #[test]
    fn gradual_shift_earlier() {
        let mut mapper = mapper();
        mapper.commit_baseline(Duration::ZERO);
        mapper.map(Duration::ZERO, None);
        mapper.shift(ms(100), ShiftDirection::Earlier, ShiftMode::Gradual);

        // 100ms media advance per packet, warp_ratio 0.01 => 1ms per packet
        assert_eq!(expect_pts(mapper.map(ms(100), None)), ms(99));
        assert_eq!(expect_pts(mapper.map(ms(200), None)), ms(198));
        for i in 3..=100 {
            mapper.map(ms(i * 100), None);
        }
        // shift fully applied, mapping is stable again
        assert_eq!(expect_pts(mapper.map(ms(10_100), None)), ms(10_000));
        assert_eq!(expect_pts(mapper.map(ms(10_200), None)), ms(10_100));
    }

    #[test]
    fn gradual_shift_later() {
        let mut mapper = mapper();
        mapper.commit_baseline(Duration::ZERO);
        mapper.map(Duration::ZERO, None);
        mapper.shift(ms(50), ShiftDirection::Later, ShiftMode::Gradual);

        assert_eq!(expect_pts(mapper.map(ms(100), None)), ms(101));
        for i in 2..=50 {
            mapper.map(ms(i * 100), None);
        }
        assert_eq!(expect_pts(mapper.map(ms(5100), None)), ms(5150));
    }

    #[test]
    fn immediate_shift_earlier_after_content_drop() {
        let mut mapper = mapper();
        mapper.commit_baseline(Duration::ZERO);
        assert_eq!(expect_pts(mapper.map(ms(1000), None)), ms(1000));

        // caller dropped (1s, 16s] of content, skipping to a keyframe
        mapper.shift(ms(15_000), ShiftDirection::Earlier, ShiftMode::Immediate);
        // 15s raw jump is not a discontinuity: it was pre-announced
        assert_eq!(expect_pts(mapper.map(ms(16_033), None)), ms(1033));
    }

    #[test]
    fn immediate_shift_later_moves_baseline_below_zero() {
        let mut mapper = mapper();
        mapper.commit_baseline(Duration::ZERO);
        assert_eq!(expect_pts(mapper.map(ms(100), None)), ms(100));

        mapper.shift(ms(500), ShiftDirection::Later, ShiftMode::Immediate);
        assert_eq!(expect_pts(mapper.map(ms(200), None)), ms(700));
    }

    #[test]
    fn playhead_inverts_the_mapping() {
        let mut mapper = mapper();
        assert_eq!(mapper.playhead(ms(10_000)), None);
        mapper.commit_baseline(ms(5000));
        assert_eq!(mapper.playhead(ms(10_000)), None);
        mapper.set_queue_offset(ms(2000));
        // consumed track pts = 10s - 2s = 8s => raw = 8s + 5s baseline
        assert_eq!(mapper.playhead(ms(10_000)), Some(ms(13_000)));

        // before the queue reaches the offset the playhead is negative
        mapper.commit_baseline(ms(500));
        assert_eq!(
            mapper.playhead_nanos(Duration::ZERO),
            Some(-(ms(1500).as_nanos() as i128))
        );
        assert_eq!(mapper.playhead(Duration::ZERO), Some(Duration::ZERO));
    }

    #[test]
    fn reset_clears_state() {
        let mut mapper = mapper();
        mapper.commit_baseline(ms(5000));
        mapper.set_queue_offset(ms(2000));
        mapper.reset();
        assert!(!mapper.is_committed());
        assert_eq!(mapper.playhead(ms(10_000)), None);
    }
}
