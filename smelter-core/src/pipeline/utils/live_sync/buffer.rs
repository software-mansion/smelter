use std::collections::VecDeque;
use std::time::Duration;

use crate::{
    pipeline::utils::input_sync::{InputSyncItem, TimestampAnchor},
    utils::live_sync::edge_estimator::EdgeEstimate,
};

use crate::prelude::*;

#[derive(Debug, Copy, Clone)]
pub(crate) enum BufferingStrategy {
    Range {
        min: Duration, // compare to lower bound
        max: Duration, // compare to upper bound
        desired: Duration,
    },
    WithSpread {
        min: Duration, // compare to lower bound
        max: Duration, // compare to lower bound
        desired: Duration,
    },
}

impl BufferingStrategy {
    pub fn desired_buffer(&self) -> Duration {
        match *self {
            BufferingStrategy::Range { desired, .. } => desired,
            BufferingStrategy::WithSpread { desired, .. } => desired,
        }
    }

    pub fn max_shift(
        &self,
        current: TimestampAnchor,
        target: TimestampAnchor,
        input_step: Duration,
    ) -> Duration {
        if current == target {
            return Duration::ZERO;
        }

        let desired = self.desired_buffer();
        let ratio = current.distance_to(target).as_secs_f64() / desired.as_secs_f64();

        if current.presents_later_than(target) {
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

    /// Anchor this strategy aims for: the mapping that gives the buffer it is
    /// configured to keep. Each variant places the bound it measures, so
    /// `Range` puts the freshest content at `desired` and `WithSpread` the
    /// oldest, leaving the batch spread on top of it.
    pub(super) fn desired_anchor(
        &self,
        estimation: EdgeEstimate,
        now_pts: Duration,
    ) -> TimestampAnchor {
        let input_pts = match *self {
            BufferingStrategy::Range { .. } => estimation.upper_bound.pts,
            BufferingStrategy::WithSpread { .. } => estimation.lower_bound.pts,
        };
        TimestampAnchor {
            input_pts,
            output_pts: now_pts + self.desired_buffer(),
        }
    }

    /// Whether the buffer `anchor` produces is within the range this strategy
    /// allows; when it is not, the anchor should be re-anchored at
    /// [`desired_anchor`](Self::desired_anchor).
    pub(super) fn buffer_in_range(
        &self,
        estimation: EdgeEstimate,
        anchor: TimestampAnchor,
        now_pts: Duration,
    ) -> bool {
        let lower_bound = anchor.to_output_pts(estimation.lower_bound.pts);
        let upper_bound = anchor.to_output_pts(estimation.upper_bound.pts);
        match *self {
            BufferingStrategy::Range { min, max, .. } => {
                lower_bound >= now_pts + min && upper_bound <= now_pts + max
            }
            BufferingStrategy::WithSpread { min, max, .. } => {
                lower_bound >= now_pts + min && lower_bound <= now_pts + max
            }
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

    /// Pts of the buffered items, in the order [`read`](Self::read) would
    /// produce them. Items held back by [`try_read`](Self::try_read) are
    /// included.
    fn pts_values(&self) -> impl Iterator<Item = Duration>;

    /// Pts of the item [`read`](Self::read) would produce; `None` only when
    /// the buffer is empty.
    fn peek_pts(&self) -> Option<Duration> {
        self.pts_values().next()
    }
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

    fn pts_values(&self) -> impl Iterator<Item = Duration> {
        self.queue.iter().map(|chunk| chunk.pts())
    }
}

pub(crate) type ChunkBuffer = FifoBuffer<EncodedInputChunk>;
