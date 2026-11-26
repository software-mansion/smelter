use std::collections::VecDeque;
use std::time::Duration;

use crate::{
    pipeline::utils::input_sync::InputSyncItem, stats::LiveSyncBufferStats,
    utils::live_sync::edge_estimator::EdgeEstimate,
};

use crate::prelude::*;

/// Buffer measured from the freshest delivered content (the upper bound).
///
/// The buffer can never be smaller than the delivery spread (upper minus
/// lower bound, i.e. jitter or the size of a batch), so if `desired` or
/// `max` do not leave room for it above `min` they are raised to fit (see
/// [`BufferingStrategy::band`]).
#[derive(Debug, Copy, Clone)]
pub(crate) enum BufferingStrategy {
    Range {
        min: Duration, // compare to lower bound
        max: Duration, // compare to upper bound
        desired: Duration,
    },
}

impl BufferingStrategy {
    pub fn desired_buffer(&self) -> Duration {
        match *self {
            BufferingStrategy::Range { desired, .. } => desired,
        }
    }

    pub fn max_shift(
        &self,
        current: TimestampOffset,
        target: TimestampOffset,
        input_step: Timestamp,
    ) -> Timestamp {
        if current == target {
            return Timestamp::ZERO;
        }

        let desired = self.desired_buffer();
        let ratio = (current - target).abs_duration().as_secs_f64() / desired.as_secs_f64();

        if current > target {
            // shrinking buffer

            // min 1% shrink
            // max 3% shrink, but it can only happen if distance is 3x of desired buffer
            let rate = 0.01 + (0.02 * f64::clamp(ratio / 3.0, 0.0, 1.0));
            input_step.mul_f64(rate)
        } else {
            // increasing buffer

            // max 4% increase, but it can only happen if distance is desired, so buffer is 0
            // min 1% increase to avoid too slow convergence
            let rate = 0.01 + (0.03 * f64::clamp(ratio, 0.0, 1.0));

            input_step.mul_f64(rate)
        }
    }

    /// Anchors this strategy accepts for `estimation`, and the one it aims for.
    ///
    /// The slowest content (lower bound) gets at least `min`, the freshest (upper bound) at most
    /// `max` and ideally `desired`. When the delivery spread does not fit, `min` wins.
    pub(super) fn band(&self, estimation: &EdgeEstimate, now_pts: Timestamp) -> Band {
        let BufferingStrategy::Range { min, max, desired } = *self;
        let (lower_bound, upper_bound) = (estimation.lower_bound, estimation.upper_bound);

        // Without a stable lower bound the spread is unknown, so the freshest content stands in
        // for the slowest.
        let slowest_pts = match lower_bound.stable {
            true => lower_bound.pts,
            false => upper_bound.pts,
        };
        let slowest_at_min = Timestamp::offset(slowest_pts, now_pts + min);

        Band {
            lower: lower_bound.stable.then_some(slowest_at_min),
            upper: TimestampOffset::max(
                Timestamp::offset(upper_bound.pts, now_pts + max),
                slowest_at_min,
            ),
            target: TimestampOffset::max(
                Timestamp::offset(upper_bound.pts, now_pts + desired),
                slowest_at_min,
            ),
        }
    }
}

/// Band of an anchor the tracks share: sized from the estimate over all their chunks, but never
/// outside the band of the priority track alone, when there is one.
pub(super) fn shared_band(
    strategy: BufferingStrategy,
    shared: &EdgeEstimate,
    priority: Option<&EdgeEstimate>,
    now_pts: Timestamp,
) -> Band {
    let band = strategy.band(shared, now_pts);
    match priority {
        Some(priority) => band.clamp_to(&strategy.band(priority, now_pts)),
        None => band,
    }
}

/// Range of anchors a [`BufferingStrategy`] accepts, and the anchor it aims for when the current
/// one falls outside of it. A larger anchor presents the same content later (more buffer).
#[derive(Debug, Clone, Copy)]
pub(super) struct Band {
    /// Smallest accepted anchor; `None` while the lower bound of the estimate is not stable (see
    /// [`PtsBound::stable`](super::edge_estimator::PtsBound::stable)), so it is not judged.
    pub lower: Option<TimestampOffset>,
    /// Largest accepted anchor.
    pub upper: TimestampOffset,
    pub target: TimestampOffset,
}

impl Band {
    pub fn contains(&self, anchor: TimestampOffset) -> bool {
        self.lower.is_none_or(|lower| anchor >= lower) && anchor <= self.upper
    }

    /// This band with every anchor moved inside `limit`.
    pub fn clamp_to(self, limit: &Band) -> Band {
        let clamp = |anchor: TimestampOffset| {
            let anchor = TimestampOffset::min(anchor, limit.upper);
            match limit.lower {
                Some(lower) => TimestampOffset::max(anchor, lower),
                None => anchor,
            }
        };
        Band {
            lower: self.lower.map(clamp),
            upper: clamp(self.upper),
            target: clamp(self.target),
        }
    }
}

/// Storage for the chunks a [`LiveSyncTrack`] holds between write and read.
/// Abstracts the buffering policy: a plain FIFO releases everything in write
/// order, while e.g. a jitter buffer can reorder out-of-order delivery and
/// hold items behind a gap that might still be filled.
///
/// Buffers are created from the type alone (`Default`); an input picks the
/// policy of its tracks by naming the buffer type.
///
/// [`LiveSyncTrack`]: super::LiveSyncTrack
pub(crate) trait LiveSyncBuffer: Default + Send + 'static {
    /// Element buffered by this buffer.
    type Chunk: InputSyncItem;

    /// Adds an item to the buffer.
    fn write(&mut self, item: Self::Chunk);

    /// Removes and returns the next item; `None` only when the buffer is
    /// empty. Implementations that hold items back (e.g. a jitter buffer
    /// waiting on a gap) give up on the missing data and return the next
    /// item they have.
    fn read(&mut self) -> Option<Self::Chunk>;

    /// Removes and returns the next item only when it is a direct
    /// continuation of the data read so far; `None` when the buffer is empty
    /// or the next item is behind a gap that might still be filled.
    fn try_read(&mut self) -> Option<Self::Chunk>;

    /// Pts of the item [`read`](Self::read) would produce; `None` only when
    /// the buffer is empty.
    fn peek_next_pts(&self) -> Option<Timestamp>;

    /// Snapshot of the buffered content for stats.
    fn stats(&self) -> LiveSyncBufferStats;
}

/// Plain FIFO buffer: items come out in write order and nothing is ever held
/// back, so [`read`](LiveSyncBuffer::read) and
/// [`try_read`](LiveSyncBuffer::try_read) behave the same.
pub(crate) struct FifoBuffer<T> {
    queue: VecDeque<T>,
}

impl<T> Default for FifoBuffer<T> {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }
}

impl<T: InputSyncItem + Send + 'static> LiveSyncBuffer for FifoBuffer<T> {
    type Chunk = T;

    fn write(&mut self, item: T) {
        self.queue.push_back(item);
    }

    fn read(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    fn try_read(&mut self) -> Option<T> {
        self.read()
    }

    fn peek_next_pts(&self) -> Option<Timestamp> {
        self.queue.front().map(|chunk| chunk.pts())
    }

    fn stats(&self) -> LiveSyncBufferStats {
        let min = self.queue.iter().map(|chunk| chunk.pts()).min();
        let max = self.queue.iter().map(|chunk| chunk.pts()).max();
        LiveSyncBufferStats::Fifo {
            duration: match (min, max) {
                (Some(min), Some(max)) => max - min,
                _ => Timestamp::ZERO,
            },
        }
    }
}

pub(crate) type ChunkBuffer = FifoBuffer<EncodedInputChunk>;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{BufferingStrategy, shared_band};
    use crate::{
        Timestamp, TimestampOffset,
        utils::live_sync::edge_estimator::{DeliveryStats, EdgeEstimate, PtsBound},
    };

    const NOW: Timestamp = Timestamp::from_secs(100);

    const STRATEGY: BufferingStrategy = BufferingStrategy::Range {
        min: Duration::from_secs(1),
        desired: Duration::from_secs(2),
        max: Duration::from_secs(5),
    };

    /// Freshest content arrives right now, the slowest `lateness` behind it.
    fn estimate(lateness: Timestamp) -> EdgeEstimate {
        EdgeEstimate {
            upper_bound: PtsBound {
                pts: NOW,
                stable: true,
            },
            lower_bound: PtsBound {
                pts: NOW - lateness,
                stable: true,
            },
            recent_upper_bound_pts: Some(NOW),
            delivery: DeliveryStats {
                last_pts: NOW,
                observed_for: Duration::from_secs(60),
                max_arrival_gap: Duration::from_millis(20),
                since_last_arrival: Duration::ZERO,
            },
        }
    }

    /// Buffer of the freshest content under `anchor`.
    fn buffer(anchor: TimestampOffset) -> Timestamp {
        anchor.to_output_pts(NOW) - NOW
    }

    #[test]
    fn without_priority_every_track_gets_min() {
        // video 6s late needs 7s of buffer, over `max`
        let shared = estimate(Timestamp::from_secs(6));
        let band = shared_band(STRATEGY, &shared, None, NOW);
        assert_eq!(buffer(band.target), Timestamp::from_secs(7));
        assert_eq!(buffer(band.upper), Timestamp::from_secs(7));
    }

    #[test]
    fn priority_caps_other_track_at_max() {
        let audio = estimate(Timestamp::from_millis(100));
        let shared = estimate(Timestamp::from_secs(6));
        let band = shared_band(STRATEGY, &shared, Some(&audio), NOW);
        assert_eq!(buffer(band.target), Timestamp::from_secs(5));
        assert!(band.contains(band.target));
        assert!(!band.contains(band.target - Timestamp::from_millis(100)));
    }

    #[test]
    fn other_track_within_max_still_counts() {
        let audio = estimate(Timestamp::from_millis(100));
        let shared = estimate(Timestamp::from_secs(2));
        let band = shared_band(STRATEGY, &shared, Some(&audio), NOW);
        assert_eq!(buffer(band.target), Timestamp::from_secs(3));
    }

    #[test]
    fn priority_track_lateness_goes_over_max() {
        let audio = estimate(Timestamp::from_secs(6));
        let shared = estimate(Timestamp::from_secs(6));
        let band = shared_band(STRATEGY, &shared, Some(&audio), NOW);
        assert_eq!(buffer(band.target), Timestamp::from_secs(7));
    }
}
