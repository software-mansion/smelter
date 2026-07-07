use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Number of buckets the sliding window is divided into.
const BUCKET_COUNT: u64 = 30;

/// Number of the most recent buckets used to measure `media_advance_rate`.
const RATE_SPAN_BUCKETS: usize = 5;

/// `media_advance_rate` below this value counts as live-locked.
const LIVE_LOCKED_MAX_RATE: f64 = 1.3;

/// How fast the long-term min delay is allowed to rise, to compensate for
/// clock drift between the source clock and the local clock.
const LONG_TERM_MIN_DRIFT_NANOS_PER_SEC: i128 = 1_000_000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveEdgeEstimatorOptions {
    /// Sliding window of the min-delay filter. Longer window gives a more
    /// stable estimate, but adapts slower to clock drift or route changes.
    pub window: Duration,
    /// No observations for this long marks the estimate as stale.
    pub stale_after: Duration,
    /// Minimal timespan covered by observations in the window before the
    /// estimate is considered reliable.
    pub warmup: Duration,
}

impl Default for LiveEdgeEstimatorOptions {
    fn default() -> Self {
        Self {
            window: Duration::from_secs(30),
            stale_after: Duration::from_secs(2),
            warmup: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveEdgeState {
    /// Not enough recent data; edge estimate is not available yet.
    Converging,
    /// Estimate is reliable.
    Tracking,
    /// No packets for at least `stale_after`. The edge is not extrapolated
    /// past the last received packet.
    Stale { elapsed: Duration },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveEdgeSnapshot {
    pub state: LiveEdgeState,
    /// Estimated newest PTS produced by the source, in the same raw
    /// (non-normalized) timestamp domain as the observed PTS.
    #[allow(dead_code)]
    pub edge: Option<Duration>,
    /// Newest PTS that actually arrived.
    pub last_arrived_pts: Option<Duration>,
    /// Spread between typical worst-case and best-case delivery delay.
    /// Approximates how much buffer the delivery pattern requires (e.g.
    /// roughly segment duration for segmented protocols).
    #[allow(dead_code)]
    pub jitter: Duration,
    /// How much the best-case delay in the current window exceeds the
    /// long-term best-case. Near zero normally; grows when every recent
    /// packet is late, which usually means local backpressure or sustained
    /// congestion. High value means `edge` underestimates the real live edge.
    pub delay_inflation: Duration,
    /// Observed PTS advance per second of wall clock over the most recent
    /// buckets. ~1.0 means delivery is locked to real time, >>1.0 means a
    /// backlog is draining (the live edge is ahead of everything received
    /// so far).
    pub media_advance_rate: Option<f64>,
}

impl LiveEdgeSnapshot {
    /// True when delivery advances at real-time rate, i.e. the newest
    /// received packet is close to the actual live edge.
    pub fn live_locked(&self) -> bool {
        !matches!(self.state, LiveEdgeState::Stale { .. })
            && matches!(self.media_advance_rate, Some(rate) if rate < LIVE_LOCKED_MAX_RATE)
    }

    /// How far behind the live edge is a track that currently plays
    /// `playhead` (in the same raw timestamp domain).
    #[allow(dead_code)]
    pub fn lag_behind(&self, playhead: Duration) -> Option<Duration> {
        match (self.state, self.edge) {
            (LiveEdgeState::Tracking, Some(edge)) => Some(edge.saturating_sub(playhead)),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Bucket {
    index: u64,
    min_delay_nanos: i128,
    max_delay_nanos: i128,
    max_pts: Duration,
}

/// Estimates the position of the live edge of a stream from per-packet
/// arrival times.
///
/// For a live source no packet can arrive before it was produced, and
/// jitter, bursts or backpressure can only inflate the arrival delay. The
/// windowed minimum of `arrival - pts` is therefore the tightest observed
/// bound on the production-to-arrival offset, and
/// `edge(now) = now - min_delay` estimates the newest PTS the source has
/// produced so far.
///
/// Feed it raw source timestamps (unit-converted to `Duration`, but not
/// normalized or shifted) via [`Self::observe`]. Both absolute origins
/// (source PTS and arrival time) are arbitrary; they cancel out in every
/// value exposed by [`Self::snapshot`].
///
/// Purely an observer; does not hold packets and has no side effects on the
/// stream. Not thread-safe on its own; wrap it in a mutex to share between
/// an input's audio and video threads.
#[derive(Debug)]
pub(crate) struct LiveEdgeEstimator {
    options: LiveEdgeEstimatorOptions,
    bucket_duration: Duration,
    /// Instant of the first observation; arrival times are stored relative
    /// to it.
    sync_point: Option<Instant>,
    /// Non-empty buckets, ascending by index; may be sparse.
    buckets: VecDeque<Bucket>,
    last_recv_time: Duration,
    max_pts: Duration,
    /// `(value_nanos, as_of_arrival)`, rises at most
    /// `LONG_TERM_MIN_DRIFT_NANOS_PER_SEC`.
    long_term_min: Option<(i128, Duration)>,
}

impl LiveEdgeEstimator {
    pub fn new(options: LiveEdgeEstimatorOptions) -> Self {
        Self {
            bucket_duration: Duration::max(
                options.window / BUCKET_COUNT as u32,
                Duration::from_millis(1),
            ),
            options,
            sync_point: None,
            buckets: VecDeque::new(),
            last_recv_time: Duration::ZERO,
            max_pts: Duration::ZERO,
            long_term_min: None,
        }
    }

    /// Records one packet. `pts` must be the raw source timestamp and
    /// `recv_time` should be captured as close to the network read as
    /// possible, before any blocking send downstream.
    pub fn observe(&mut self, pts: Duration, recv_time: Instant) {
        let sync_point = *self.sync_point.get_or_insert(recv_time);
        // recv_time but as duration since sync_point
        let recv_time = recv_time.saturating_duration_since(sync_point);
        let delay = recv_time.as_nanos() as i128 - pts.as_nanos() as i128;

        let mut index = (recv_time.as_nanos() / self.bucket_duration.as_nanos()) as u64;
        if let Some(back) = self.buckets.back() {
            // out-of-order arrivals land in the newest bucket
            index = u64::max(index, back.index);
        }
        match self.buckets.back_mut() {
            Some(back) if back.index == index => {
                back.min_delay_nanos = i128::min(back.min_delay_nanos, delay);
                back.max_delay_nanos = i128::max(back.max_delay_nanos, delay);
                back.max_pts = Duration::max(back.max_pts, pts);
            }
            _ => self.buckets.push_back(Bucket {
                index,
                min_delay_nanos: delay,
                max_delay_nanos: delay,
                max_pts: pts,
            }),
        }
        while let Some(front) = self.buckets.front() {
            if front.index + BUCKET_COUNT <= index {
                self.buckets.pop_front();
            } else {
                break;
            }
        }

        self.last_recv_time = Duration::max(self.last_recv_time, recv_time);
        self.max_pts = Duration::max(self.max_pts, pts);

        let long_term = match self.long_term_min {
            Some((min_drift, min_recv_time)) => {
                let drift = recv_time.saturating_sub(min_recv_time).as_nanos() as i128
                    * LONG_TERM_MIN_DRIFT_NANOS_PER_SEC
                    / 1_000_000_000;
                i128::min(min_drift + drift, delay)
            }
            None => delay,
        };
        self.long_term_min = Some((long_term, recv_time));
    }

    pub fn snapshot(&self, now: Instant) -> LiveEdgeSnapshot {
        let Some(sync_point) = self.sync_point else {
            return LiveEdgeSnapshot {
                state: LiveEdgeState::Converging,
                edge: None,
                last_arrived_pts: None,
                jitter: Duration::ZERO,
                delay_inflation: Duration::ZERO,
                media_advance_rate: None,
            };
        };
        let now = now.saturating_duration_since(sync_point);
        let now_index = (now.as_nanos() / self.bucket_duration.as_nanos()) as u64;
        let valid: Vec<&Bucket> = self
            .buckets
            .iter()
            .filter(|bucket| bucket.index + BUCKET_COUNT > now_index)
            .collect();

        let window_min = valid.iter().map(|b| b.min_delay_nanos).min();

        let jitter = match window_min {
            Some(min) => {
                let mut max_delays: Vec<i128> = valid.iter().map(|b| b.max_delay_nanos).collect();
                max_delays.sort_unstable();
                saturating_duration_from_nanos(max_delays[max_delays.len() / 2] - min)
            }
            None => Duration::ZERO,
        };

        let delay_inflation = match (window_min, self.long_term_min) {
            (Some(min), Some((value, as_of))) => {
                let drift = now.saturating_sub(as_of).as_nanos() as i128
                    * LONG_TERM_MIN_DRIFT_NANOS_PER_SEC
                    / 1_000_000_000;
                saturating_duration_from_nanos(min - (value + drift))
            }
            _ => Duration::ZERO,
        };

        let rate_span = &valid[valid.len().saturating_sub(RATE_SPAN_BUCKETS)..];
        let media_advance_rate = match (rate_span.first(), rate_span.last()) {
            (Some(first), Some(last)) if last.index > first.index => {
                let pts_diff = last.max_pts.saturating_sub(first.max_pts);
                let elapsed = (last.index - first.index) as u32 * self.bucket_duration;
                Some(pts_diff.as_secs_f64() / elapsed.as_secs_f64())
            }
            _ => None,
        };

        let silence = now.saturating_sub(self.last_recv_time);
        let warmed_up = match (valid.first(), valid.last()) {
            (Some(first), Some(last)) => {
                (last.index - first.index + 1) as u32 * self.bucket_duration >= self.options.warmup
            }
            _ => false,
        };
        let state = if silence > self.options.stale_after {
            LiveEdgeState::Stale { elapsed: silence }
        } else if !warmed_up {
            LiveEdgeState::Converging
        } else {
            LiveEdgeState::Tracking
        };

        let edge = match (state, window_min) {
            (LiveEdgeState::Tracking, Some(min)) => Some(Duration::max(
                saturating_duration_from_nanos(now.as_nanos() as i128 - min),
                self.max_pts,
            )),
            (LiveEdgeState::Stale { .. }, _) => Some(self.max_pts),
            _ => None,
        };

        LiveEdgeSnapshot {
            state,
            edge,
            last_arrived_pts: Some(self.max_pts),
            jitter,
            delay_inflation,
            media_advance_rate,
        }
    }

    /// Hard reset of all state. Call on stream discontinuity (a PTS jump
    /// past the discontinuity threshold means the raw timestamp domain
    /// changed and old observations are meaningless).
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.sync_point = None;
        self.buckets.clear();
        self.last_recv_time = Duration::ZERO;
        self.max_pts = Duration::ZERO;
        self.long_term_min = None;
    }
}

pub(super) fn saturating_duration_from_nanos(nanos: i128) -> Duration {
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

    fn estimator() -> LiveEdgeEstimator {
        LiveEdgeEstimator::new(LiveEdgeEstimatorOptions::default())
    }

    #[test]
    fn no_data() {
        let estimator = estimator();
        let snapshot = estimator.snapshot(Instant::now());
        assert_eq!(snapshot.state, LiveEdgeState::Converging);
        assert_eq!(snapshot.edge, None);
        assert_eq!(snapshot.last_arrived_pts, None);
        assert!(!snapshot.live_locked());
        assert_eq!(snapshot.lag_behind(Duration::ZERO), None);
    }

    #[test]
    fn converging_before_warmup() {
        let start = Instant::now();
        let mut estimator = estimator();
        // 1s of data, warmup is 2s
        for i in 0..30 {
            estimator.observe(ms(i * 33), start + ms(i * 33));
        }
        let snapshot = estimator.snapshot(start + ms(990));
        assert_eq!(snapshot.state, LiveEdgeState::Converging);
        assert_eq!(snapshot.edge, None);
    }

    #[test]
    fn steady_stream_tracks_edge() {
        let start = Instant::now();
        let mut estimator = estimator();
        // 5s of a jitterless 30fps stream with a constant transport delay;
        // the delay is unobservable and absorbed by the epoch
        for i in 0..150 {
            estimator.observe(ms(i * 33), start + ms(100 + i * 33));
        }
        let last_pts = ms(149 * 33);
        let snapshot = estimator.snapshot(start + ms(100 + 149 * 33));

        assert_eq!(snapshot.state, LiveEdgeState::Tracking);
        assert_eq!(snapshot.edge, Some(last_pts));
        assert_eq!(snapshot.last_arrived_pts, Some(last_pts));
        assert_eq!(snapshot.jitter, Duration::ZERO);
        assert_eq!(snapshot.delay_inflation, Duration::ZERO);
        assert!(snapshot.live_locked());
        assert_eq!(
            snapshot.lag_behind(last_pts.saturating_sub(ms(2000))),
            Some(ms(2000))
        );

        // edge keeps advancing between packets
        let snapshot = estimator.snapshot(start + ms(100 + 149 * 33 + 500));
        assert_eq!(snapshot.edge, Some(last_pts + ms(500)));
    }

    #[test]
    fn jitter_measured_from_delay_spread() {
        let start = Instant::now();
        let mut estimator = estimator();
        // delay alternates between +0ms and +50ms
        for i in 0..150 {
            let jitter = if i % 2 == 0 { 0 } else { 50 };
            estimator.observe(ms(i * 33), start + ms(i * 33 + jitter));
        }
        let snapshot = estimator.snapshot(start + ms(149 * 33 + 50));
        assert_eq!(snapshot.state, LiveEdgeState::Tracking);
        assert_eq!(snapshot.jitter, ms(50));
    }

    #[test]
    fn backlog_drain_detected_then_live_locked() {
        let start = Instant::now();
        let mut estimator = estimator();
        // 10s of media delivered in 3s (draining a server-side backlog)
        for i in 0..300 {
            estimator.observe(ms(i * 33), start + ms(i * 10));
        }
        let snapshot = estimator.snapshot(start + ms(3000));
        assert!(snapshot.media_advance_rate.unwrap() > 2.0);
        assert!(!snapshot.live_locked());

        // then delivery continues at real-time rate
        for i in 0..300 {
            estimator.observe(ms(9900 + i * 33), start + ms(3000 + i * 33));
        }
        let snapshot = estimator.snapshot(start + ms(3000 + 299 * 33));
        let rate = snapshot.media_advance_rate.unwrap();
        assert!((0.8..1.2).contains(&rate), "rate: {rate}");
        assert!(snapshot.live_locked());
    }

    #[test]
    fn stale_freezes_edge() {
        let start = Instant::now();
        let mut estimator = estimator();
        for i in 0..100 {
            estimator.observe(ms(i * 33), start + ms(i * 33));
        }
        let last_pts = ms(99 * 33);

        let snapshot = estimator.snapshot(start + ms(99 * 33 + 5000));
        assert!(matches!(snapshot.state, LiveEdgeState::Stale { .. }));
        assert_eq!(snapshot.edge, Some(last_pts));
        assert!(!snapshot.live_locked());
        assert_eq!(snapshot.lag_behind(Duration::ZERO), None);
    }

    #[test]
    fn delay_inflation_detects_sustained_congestion() {
        let start = Instant::now();
        let mut estimator = estimator();
        // 40s with stable delivery
        for i in 0..1200 {
            estimator.observe(ms(i * 33), start + ms(i * 33));
        }
        // 40s with every packet 500ms late; the cheap buckets slide out of
        // the 30s window while the long-term min persists
        for i in 1200..2400 {
            estimator.observe(ms(i * 33), start + ms(i * 33 + 500));
        }
        let snapshot = estimator.snapshot(start + ms(2399 * 33 + 500));
        assert_eq!(snapshot.state, LiveEdgeState::Tracking);
        assert!(
            snapshot.delay_inflation > ms(400),
            "inflation: {:?}",
            snapshot.delay_inflation
        );
    }

    #[test]
    fn reset_clears_state() {
        let start = Instant::now();
        let mut estimator = estimator();
        for i in 0..300 {
            estimator.observe(ms(i * 33), start + ms(i * 33));
        }
        estimator.reset();
        let snapshot = estimator.snapshot(start + ms(300 * 33));
        assert_eq!(snapshot.state, LiveEdgeState::Converging);
        assert_eq!(snapshot.edge, None);
        assert_eq!(snapshot.last_arrived_pts, None);
    }
}
