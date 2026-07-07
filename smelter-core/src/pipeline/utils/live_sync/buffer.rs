use std::collections::VecDeque;
use std::time::{Duration, Instant};

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

pub(super) enum BufferCheckResult {
    Ok,
    TooSmall,
    TooLarge,
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
            // max 3% increase, but it can only happen if distance is 3x of desired buffer
            let rate = 0.01 + (0.02 * f64::clamp(ratio / 3.0, 0.0, 1.0));
            return input_step.mul_f64(rate);
        } else {
            // increasing buffer

            // max 4% increase, but it can only happen if distance is desired, so buffer is 0
            // min 1% increase to avoid to slow convergance
            let rate = 0.01 + (0.03 * f64::clamp(ratio, 0.0, 1.0));

            return input_step.mul_f64(rate);
        }
    }

    pub(super) fn check(
        &self,
        estimation: EdgeEstimate,
        anchor: TimestampAnchor,
        sync_point: Instant,
    ) -> BufferCheckResult {
        let now_pts = sync_point.elapsed();
        let lower_bound = anchor.to_output_pts(estimation.lower_bound.pts);
        let upper_bound = anchor.to_output_pts(estimation.upper_bound.pts);
        match *self {
            BufferingStrategy::Range { min, max, .. } => {
                if lower_bound < now_pts + min {
                    return BufferCheckResult::TooSmall;
                }

                if upper_bound > now_pts + max {
                    return BufferCheckResult::TooLarge;
                }

                return BufferCheckResult::Ok;
            }
            BufferingStrategy::WithSpread { min, max, .. } => {
                if lower_bound < now_pts + min {
                    return BufferCheckResult::TooSmall;
                }

                if lower_bound > now_pts + max {
                    return BufferCheckResult::TooLarge;
                }

                return BufferCheckResult::Ok;
            }
        }
    }
}

/// Storage for the chunks a [`LiveSyncTrack`] holds between write and read.
/// Abstracts the buffering policy: a plain FIFO releases everything in write
/// order, while e.g. a jitter buffer can reorder out-of-order delivery and
/// hold items behind a gap that might still be filled.
///
/// Buffers are created from the type alone (`Default`); a track is spawned by
/// naming its buffer type.
///
/// [`LiveSyncTrack`]: super::LiveSyncTrack
pub(crate) trait LiveSyncBuffer: Default {
    /// Element buffered by this buffer.
    type Item: InputSyncItem;

    /// Adds an item to the buffer.
    fn write(&mut self, item: Self::Item);

    /// Removes and returns the next item; `None` only when the buffer is
    /// empty. Implementations that hold items back (e.g. a jitter buffer
    /// waiting on a gap) give up on the missing data and return the next
    /// item they have.
    fn read(&mut self) -> Option<Self::Item>;

    /// Removes and returns the next item only when it is a direct
    /// continuation of the data read so far; `None` when the buffer is empty
    /// or the next item is behind a gap that might still be filled.
    fn try_read(&mut self) -> Option<Self::Item>;

    /// Returns the item [`read`](Self::read) would produce, without removing
    /// it; `None` only when the buffer is empty. Items held back by
    /// [`try_read`](Self::try_read) are still reported.
    fn peek(&self) -> Option<&Self::Item>;

    /// Largest raw pts removed by [`read`](Self::read) or
    /// [`try_read`](Self::try_read) so far (not the last one, so decode order
    /// and buffers releasing out of order do not matter); `None` before
    /// anything was released. Unmapped, like [`peek`](Self::peek), so the
    /// caller applies its own anchor.
    fn max_read_pts(&self) -> Option<Duration>;
}

/// Plain FIFO buffer: items come out in write order and nothing is ever held
/// back, so [`read`](LiveSyncBuffer::read) and
/// [`try_read`](LiveSyncBuffer::try_read) behave the same.
#[derive(Default)]
pub(crate) struct ChunkBuffer {
    queue: VecDeque<EncodedInputChunk>,
    max_read_pts: Option<Duration>,
}

impl LiveSyncBuffer for ChunkBuffer {
    type Item = EncodedInputChunk;

    fn write(&mut self, item: EncodedInputChunk) {
        self.queue.push_back(item);
    }

    fn read(&mut self) -> Option<EncodedInputChunk> {
        let item = self.queue.pop_front()?;
        self.max_read_pts = Some(match self.max_read_pts {
            Some(previous) => Duration::max(previous, item.pts()),
            None => item.pts(),
        });
        Some(item)
    }

    fn try_read(&mut self) -> Option<EncodedInputChunk> {
        self.read()
    }

    fn peek(&self) -> Option<&EncodedInputChunk> {
        self.queue.front()
    }

    fn max_read_pts(&self) -> Option<Duration> {
        self.max_read_pts
    }
}
