use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Width of one time bucket; the granularity at which old gaps expire.
const TIME_BUCKET: Duration = Duration::from_secs(10);

/// Number of time buckets kept. Only buckets with recorded gaps are stored, so they span 5 minutes
/// of continuous delivery and longer when delivery pauses.
const TIME_BUCKETS: usize = 30;

/// Gaps longer than this are not segments (stalls, reconnects) and are not recorded.
const MAX_GAP: Duration = Duration::from_secs(30);

/// Upper edge of the first gap class; everything shorter (bursts) shares it.
const MIN_GAP: Duration = Duration::from_millis(5);

/// Gap classes are log-spaced, each `2^(1/4)` wider than the previous one.
const CLASSES_PER_OCTAVE: f64 = 4.0;

/// Enough classes to cover gaps from [`MIN_GAP`] to [`MAX_GAP`].
const GAP_CLASSES: usize = 52;

/// Adjacent classes compared together, so gaps of one segment that straddle a class edge are not
/// split. Covers a ×1.41 spread of gap sizes.
const CLASSES_PER_WINDOW: usize = 3;

/// Gap time the winning classes need to hold before the estimate is trusted.
const MIN_EVIDENCE: Duration = Duration::from_secs(5);

/// Estimates the gap between delivery bursts (e.g. the HLS segment interval) from arrival times
/// alone, so pts discontinuities and resets do not affect it.
///
/// Every gap between consecutive arrivals is sorted into a log-spaced size class, which sums the
/// gap durations. Gaps tile wall-clock time, so a class sum is how much time was spent in gaps
/// of that size. The estimate is the mean gap of the adjacent classes that hold the most time:
/// - Inside a burst there are many gaps, but they are close to zero and add up to little time.
/// - One-off gaps of irregular size (stalls, a paused source) spread over many classes and win
///   only if they take up more time than the regular gap.
#[allow(dead_code)]
pub(crate) struct SegmentEstimator {
    /// Instant that time bucket indices count from.
    start: Instant,
    last_observation: Option<Instant>,
    /// Oldest first; only buckets with recorded gaps.
    time_buckets: VecDeque<TimeBucket>,
    /// Sum over all `time_buckets`.
    totals: [GapClass; GAP_CLASSES],
}

struct TimeBucket {
    index: u64,
    classes: [GapClass; GAP_CLASSES],
}

#[derive(Clone, Copy)]
struct GapClass {
    sum: Duration,
    count: u32,
}

impl GapClass {
    const EMPTY: GapClass = GapClass {
        sum: Duration::ZERO,
        count: 0,
    };
}

#[allow(dead_code)]
impl SegmentEstimator {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            last_observation: None,
            time_buckets: VecDeque::new(),
            totals: [GapClass::EMPTY; GAP_CLASSES],
        }
    }

    /// Record a chunk arriving now.
    pub fn observe(&mut self) {
        let now = Instant::now();
        let Some(last_observation) = self.last_observation.replace(now) else {
            return;
        };
        let gap = now.saturating_duration_since(last_observation);
        if gap > MAX_GAP {
            return;
        }

        let index =
            (now.saturating_duration_since(self.start).as_nanos() / TIME_BUCKET.as_nanos()) as u64;
        if self.time_buckets.back().is_none_or(|bucket| bucket.index != index) {
            self.time_buckets.push_back(TimeBucket {
                index,
                classes: [GapClass::EMPTY; GAP_CLASSES],
            });
        }
        if self.time_buckets.len() > TIME_BUCKETS
            && let Some(oldest) = self.time_buckets.pop_front()
        {
            for (total, class) in self.totals.iter_mut().zip(oldest.classes) {
                total.sum -= class.sum;
                total.count -= class.count;
            }
        }

        let bucket = self.time_buckets.back_mut().expect("never empty");
        let class_index = gap_class(gap);
        for class in [&mut bucket.classes[class_index], &mut self.totals[class_index]] {
            class.sum += gap;
            class.count += 1;
        }
    }

    /// Typical gap between delivery bursts; `None` until enough gaps were observed.
    pub fn estimate(&self) -> Option<Duration> {
        let best = self
            .totals
            .windows(CLASSES_PER_WINDOW)
            .max_by_key(|window| window.iter().map(|class| class.sum).sum::<Duration>())?;
        let sum: Duration = best.iter().map(|class| class.sum).sum();
        if sum < MIN_EVIDENCE {
            return None;
        }
        let count: u32 = best.iter().map(|class| class.count).sum();
        Some(sum / count)
    }
}

fn gap_class(gap: Duration) -> usize {
    if gap < MIN_GAP {
        return 0;
    }
    let octaves = f64::log2(gap.as_secs_f64() / MIN_GAP.as_secs_f64());
    usize::min(1 + (octaves * CLASSES_PER_OCTAVE) as usize, GAP_CLASSES - 1)
}
