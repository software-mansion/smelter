use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use super::live_edge_estimator::{LiveEdgeEstimator, LiveEdgeEstimatorOptions};
use super::track_time_mapper::{
    MapOutcome, ShiftDirection, ShiftMode, TrackTimeMapper, TrackTimeMapperOptions,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncOptions {
    /// Target buffer fill (distance between the newest received packet and
    /// the playback position); the safety margin against network stalls.
    /// It is also how far behind the live edge the input plays.
    pub target_buffer: Duration,
    /// Fill below this triggers an immediate re-anchor back to
    /// `target_buffer` (one-time freeze instead of constant starvation).
    pub min_buffer: Duration,
    /// Fill above `target_buffer` by more than this is corrected silently
    /// by playing slightly faster.
    pub gradual_threshold: Duration,
    /// Fill above `target_buffer` by more than this is corrected by
    /// dropping content up to the next keyframe (visible cut).
    pub skip_threshold: Duration,
    /// Max probe length; after this the join happens with whatever was
    /// measured.
    pub probe_cap: Duration,
    /// How often steady-state corrections are evaluated.
    pub check_interval: Duration,
    /// Consecutive out-of-band checks required before acting.
    pub sustained_checks: u32,
    /// Corrections are suspended when `delay_inflation` exceeds this: every
    /// recent packet arrived late, which means local backpressure or
    /// sustained congestion, and reacting to it would only add latency.
    pub max_delay_inflation: Duration,
    pub estimator: LiveEdgeEstimatorOptions,
    pub mapper: TrackTimeMapperOptions,
}

impl Default for LiveSyncOptions {
    fn default() -> Self {
        Self {
            target_buffer: Duration::from_secs(2),
            min_buffer: Duration::from_millis(500),
            gradual_threshold: Duration::from_millis(500),
            skip_threshold: Duration::from_secs(3),
            probe_cap: Duration::from_secs(2),
            check_interval: Duration::from_millis(500),
            sustained_checks: 3,
            max_delay_inflation: Duration::from_millis(500),
            estimator: LiveEdgeEstimatorOptions::default(),
            mapper: TrackTimeMapperOptions::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapDecision {
    Send {
        pts: Duration,
        dts: Option<Duration>,
    },
    Drop,
    /// Raw PTS jumped; the stream needs a restart (re-probe + new track).
    Discontinuity,
}

/// Ties the live-edge estimator and the track-time mapper into the
/// probe -> join -> live flow of a live input.
///
/// Usage: call [`Self::observe`] for every incoming packet from the moment
/// the connection is established (while the caller holds packets in a
/// `GopBuffer`). Once [`Self::ready_to_join`], pick the join point (newest
/// held keyframe), call [`Self::join`], register the queue track with the
/// returned offset and flush held packets through [`Self::map_video`] /
/// [`Self::map_audio`]. From then on every packet goes through the same map
/// calls, which also evaluate the steady-state corrections.
///
/// Corrections keep the buffer fill (newest received packet vs playback
/// position) at `target_buffer`:
///
/// - fill below `min_buffer` -> immediate shift later back to target
///   (one-time freeze; protects against permanent starvation after
///   publisher pauses or network stalls),
/// - fill moderately above target -> gradual, invisible catch-up,
/// - fill far above target -> drop until the next keyframe and jump.
///
/// Single-threaded; RTMP-style inputs where all events arrive on one
/// thread can use it directly, otherwise wrap in a mutex.
pub(crate) struct LiveSyncController {
    options: LiveSyncOptions,
    estimator: LiveEdgeEstimator,
    mapper: TrackTimeMapper,
    probe_start: Option<Instant>,
    joined: bool,
    /// Raw PTS the current skip started at (last mapped packet).
    skipping_from: Option<Duration>,
    last_check: Option<Instant>,
    low_streak: u32,
    high_streak: u32,
}

impl LiveSyncController {
    pub fn new(options: LiveSyncOptions) -> Self {
        Self {
            estimator: LiveEdgeEstimator::new(options.estimator),
            mapper: TrackTimeMapper::new(options.mapper),
            options,
            probe_start: None,
            joined: false,
            skipping_from: None,
            last_check: None,
            low_streak: 0,
            high_streak: 0,
        }
    }

    /// Feed every incoming packet, in every phase, with its raw PTS.
    pub fn observe(&mut self, pts: Duration, recv_time: Instant) {
        self.probe_start.get_or_insert(recv_time);
        self.estimator.observe(pts, recv_time);
    }

    /// True once the probe learned where the live edge is (delivery locked
    /// to real time) or `probe_cap` expired. Requires at least one
    /// observation.
    pub fn ready_to_join(&self, now: Instant) -> bool {
        let Some(probe_start) = self.probe_start else {
            return false;
        };
        now.saturating_duration_since(probe_start) >= self.options.probe_cap
            || self.estimator.snapshot(now).live_locked()
    }

    /// Commits `baseline` (the join point, e.g. newest held keyframe) as
    /// track time zero. Returns the offset to register the queue track
    /// with; `queue_now` is the queue position at this moment
    /// (`effective_last_pts()`).
    pub fn join(&mut self, baseline: Duration, queue_now: Duration) -> Duration {
        self.mapper.commit_baseline(baseline);
        let offset = queue_now + self.options.target_buffer;
        self.mapper.set_queue_offset(offset);
        self.joined = true;
        info!(?baseline, ?offset, "Joined live stream");
        offset
    }

    pub fn map_video(
        &mut self,
        pts: Duration,
        dts: Option<Duration>,
        keyframe: bool,
        now: Instant,
        queue_now: Duration,
    ) -> MapDecision {
        self.maybe_check(now, queue_now);
        if let Some(from) = self.skipping_from {
            if !keyframe {
                return MapDecision::Drop;
            }
            let skipped = pts.saturating_sub(from);
            if !skipped.is_zero() {
                self.mapper
                    .shift(skipped, ShiftDirection::Earlier, ShiftMode::Immediate);
                info!(?skipped, "Skipped content to catch up with the live edge");
            }
            self.skipping_from = None;
        }
        self.map(pts, dts)
    }

    pub fn map_audio(&mut self, pts: Duration, now: Instant, queue_now: Duration) -> MapDecision {
        self.maybe_check(now, queue_now);
        if self.skipping_from.is_some() {
            return MapDecision::Drop;
        }
        self.map(pts, None)
    }

    fn map(&mut self, pts: Duration, dts: Option<Duration>) -> MapDecision {
        match self.mapper.map(pts, dts) {
            MapOutcome::Pts { pts, dts } => MapDecision::Send { pts, dts },
            MapOutcome::BeforeBaseline => MapDecision::Drop,
            MapOutcome::Discontinuity => MapDecision::Discontinuity,
        }
    }

    fn maybe_check(&mut self, now: Instant, queue_now: Duration) {
        if !self.joined || self.skipping_from.is_some() {
            return;
        }
        match self.last_check {
            Some(last) if now.saturating_duration_since(last) < self.options.check_interval => {
                return;
            }
            _ => self.last_check = Some(now),
        }

        let snapshot = self.estimator.snapshot(now);
        let (Some(last_arrived), Some(playhead_nanos)) = (
            snapshot.last_arrived_pts,
            self.mapper.playhead_nanos(queue_now),
        ) else {
            return;
        };
        if snapshot.delay_inflation > self.options.max_delay_inflation {
            debug!(
                delay_inflation = ?snapshot.delay_inflation,
                "Packets arrive with inflated delay (backpressure or congestion), \
                 suspending live sync corrections"
            );
            self.low_streak = 0;
            self.high_streak = 0;
            return;
        }

        // negative when the playback position is ahead of received data
        // (starvation)
        let fill_nanos = last_arrived.as_nanos() as i128 - playhead_nanos;
        let target_nanos = self.options.target_buffer.as_nanos() as i128;
        // where the fill will settle after pending gradual corrections
        let projected_fill_nanos = fill_nanos - self.mapper.pending_shift_nanos();
        let high_bar = target_nanos + self.options.gradual_threshold.as_nanos() as i128;

        if fill_nanos < self.options.min_buffer.as_nanos() as i128 {
            self.low_streak += 1;
            self.high_streak = 0;
            if self.low_streak >= self.options.sustained_checks {
                let delta = saturating_duration(target_nanos - fill_nanos);
                self.mapper.cancel_pending_shift();
                self.mapper
                    .shift(delta, ShiftDirection::Later, ShiftMode::Immediate);
                warn!(
                    fill_ms = fill_nanos / 1_000_000,
                    ?delta,
                    "Input close to starvation, adding latency"
                );
                self.low_streak = 0;
            }
        } else if projected_fill_nanos > high_bar {
            self.low_streak = 0;
            self.high_streak += 1;
            if self.high_streak >= self.options.sustained_checks {
                let excess = saturating_duration(projected_fill_nanos - target_nanos);
                if excess > self.options.skip_threshold {
                    self.mapper.cancel_pending_shift();
                    self.skipping_from = Some(self.mapper.last_raw_pts());
                    info!(
                        fill_ms = fill_nanos / 1_000_000,
                        "Too far behind the live edge, skipping to next keyframe"
                    );
                } else {
                    self.mapper
                        .shift(excess, ShiftDirection::Earlier, ShiftMode::Gradual);
                    debug!(
                        fill_ms = fill_nanos / 1_000_000,
                        ?excess,
                        "Drifted behind the live edge, catching up gradually"
                    );
                }
                self.high_streak = 0;
            }
        } else {
            self.low_streak = 0;
            self.high_streak = 0;
        }
    }
}

fn saturating_duration(nanos: i128) -> Duration {
    match u64::try_from(nanos) {
        Ok(nanos) => Duration::from_nanos(nanos),
        Err(_) if nanos < 0 => Duration::ZERO,
        Err(_) => Duration::from_nanos(u64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    /// Drives a controller through probe + join on a steady 30fps stream
    /// where the packet with pts `i * 33ms` arrives `i * 33ms` after start,
    /// with the queue clock running in lockstep with arrival time.
    struct Harness {
        start: Instant,
        controller: LiveSyncController,
        offset: Duration,
        baseline: Duration,
    }

    impl Harness {
        fn join(mut controller: LiveSyncController) -> Self {
            let start = Instant::now();
            let mut i = 0;
            while !controller.ready_to_join(start + ms(i * 33)) {
                controller.observe(ms(i * 33), start + ms(i * 33));
                i += 1;
                assert!(i < 1000, "probe never ended");
            }
            let baseline = ms((i - 1) * 33);
            let offset = controller.join(baseline, ms(i * 33));
            Self {
                start,
                controller,
                offset,
                baseline,
            }
        }

        /// Runs `packets` video packets with pts starting at `from_pts`,
        /// arriving at wall time `from_wall + i * 33ms` (queue clock equals
        /// wall time since start). Keyframe every 30 packets.
        fn run(
            &mut self,
            from_pts: Duration,
            from_wall: Duration,
            packets: u64,
        ) -> Vec<(Duration, MapDecision)> {
            let mut decisions = Vec::new();
            for i in 0..packets {
                let pts = from_pts + ms(i * 33);
                let wall = from_wall + ms(i * 33);
                let recv_time = self.start + wall;
                self.controller.observe(pts, recv_time);
                let decision = self.controller.map_video(
                    pts,
                    Some(pts),
                    i % 30 == 0,
                    recv_time,
                    wall,
                );
                decisions.push((pts, decision));
            }
            decisions
        }
    }

    fn expect_send(decision: &MapDecision) -> Duration {
        match decision {
            MapDecision::Send { pts, .. } => *pts,
            other => panic!("expected Send, got {other:?}"),
        }
    }

    #[test]
    fn ready_to_join_needs_data() {
        let controller = LiveSyncController::new(LiveSyncOptions::default());
        assert!(!controller.ready_to_join(Instant::now()));
    }

    #[test]
    fn joins_when_live_locked() {
        let start = Instant::now();
        let mut controller = LiveSyncController::new(LiveSyncOptions::default());
        for i in 0..40 {
            controller.observe(ms(i * 33), start + ms(i * 33));
        }
        // ~1.3s of steady stream: live-locked before the 2s cap
        assert!(controller.ready_to_join(start + ms(40 * 33)));
    }

    #[test]
    fn joins_at_probe_cap_during_backlog_drain() {
        let start = Instant::now();
        let mut controller = LiveSyncController::new(LiveSyncOptions::default());
        // backlog dump: media advances 10x faster than wall clock
        for i in 0..60 {
            controller.observe(ms(i * 330), start + ms(i * 33));
        }
        // not locked before the cap...
        assert!(!controller.ready_to_join(start + ms(1980)));
        // ...but the cap forces the join
        assert!(controller.ready_to_join(start + ms(2000)));
    }

    #[test]
    fn steady_stream_maps_without_corrections() {
        let mut harness = Harness::join(LiveSyncController::new(LiveSyncOptions::default()));
        let from = harness.baseline;
        let decisions = harness.run(from, from, 300);
        for (pts, decision) in decisions {
            assert_eq!(expect_send(&decision), pts - harness.baseline);
        }
    }

    #[test]
    fn adds_latency_when_starving() {
        let mut harness = Harness::join(LiveSyncController::new(LiveSyncOptions::default()));
        let from = harness.baseline;
        harness.run(from, from, 100);

        // publisher pauses for 4s and resumes with continuous pts: the queue
        // kept playing, so everything now arrives behind the playback
        // position
        let from = from + ms(100 * 33);
        let pause = ms(4000);
        let decisions = harness.run(from, from + pause, 300);

        // after the correction, mapped pts must land back ahead of the
        // playback position (playhead in track time = queue_now - offset)
        let (pts, decision) = decisions.last().unwrap();
        let queue_now = *pts + pause;
        let playhead_track = queue_now - harness.offset;
        assert!(
            expect_send(decision) > playhead_track,
            "content still behind the playback position"
        );
    }

    #[test]
    fn skips_to_keyframe_when_too_far_behind() {
        // faster cadence so the test converges quickly
        let options = LiveSyncOptions {
            check_interval: ms(100),
            sustained_checks: 2,
            ..Default::default()
        };
        let mut harness = Harness::join(LiveSyncController::new(options));
        let from = harness.baseline;
        harness.run(from, from, 100);

        // pts jump 8s ahead (below the 10s discontinuity threshold) while
        // wall clock continues normally: fill is suddenly 8s too large
        let from_wall = from + ms(100 * 33);
        let from_pts = from_wall + ms(8000);
        let decisions = harness.run(from_pts, from_wall, 600);

        let drops = decisions
            .iter()
            .filter(|(_, d)| *d == MapDecision::Drop)
            .count();
        assert!(drops > 0, "expected catch-up skips");

        // skips must have brought mapped pts down close to the playback
        // position: without them the final packet would map 8s ahead
        let (pts, decision) = decisions.last().unwrap();
        let unshifted = *pts - harness.baseline;
        let mapped = expect_send(decision);
        assert!(
            unshifted - mapped > ms(4000),
            "expected at least 4s of content skipped, got {:?}",
            unshifted - mapped
        );
    }
}
