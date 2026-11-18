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
/// `max` do not leave room for it above `min` they are raised to fit.
#[derive(Debug, Copy, Clone)]
pub(crate) enum BufferingStrategy {
    Range {
        min: Duration, // compare to lower bound
        max: Duration, // compare to upper bound
        desired: Duration,
    },
}

/// Track whose latency the shared buffer protects when the tracks need different buffers.
#[derive(Debug, Copy, Clone)]
pub(crate) enum TrackPriority {
    /// The buffer grows until both tracks fit. E.g. tracks delivered over one connection (RTMP)
    /// are delayed together, so dropping one would not help the other.
    None,
    /// The buffer is sized for audio; video that does not fit within `max` is dropped.
    Audio,
    /// The buffer is sized for video; audio that does not fit within `max` is dropped.
    #[allow(dead_code)]
    Video,
}

impl TrackPriority {
    /// Estimate of the priority track; `None` without priority or without that track's estimate.
    pub(super) fn select<'a>(
        self,
        audio: Option<&'a EdgeEstimate>,
        video: Option<&'a EdgeEstimate>,
    ) -> Option<&'a EdgeEstimate> {
        match self {
            TrackPriority::None => None,
            TrackPriority::Audio => audio,
            TrackPriority::Video => video,
        }
    }
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

    /// Anchor this strategy aims for: the upper bound at `desired`, or further back if the
    /// lower bound would be less than `min` ahead.
    ///
    /// `priority` is the estimate of the track whose latency is protected (see
    /// [`TrackPriority`]).
    pub(super) fn desired_anchor(
        &self,
        estimation: &EdgeEstimate,
        priority: Option<&EdgeEstimate>,
        now_pts: Timestamp,
    ) -> TimestampOffset {
        let BufferingStrategy::Range { min, desired, max } = *self;

        // optimal choice: upper bound (the most optimistic packets) with `desired` buffer
        let mut offset = Timestamp::offset(estimation.upper_bound.pts, now_pts + desired);

        // lower bound (the most pessimistic packets) with at least `min` buffer
        offset = TimestampOffset::max(
            offset,
            Timestamp::offset(estimation.lower_bound.pts, now_pts + min),
        );

        if let Some(priority) = priority {
            // the other track may not shift the priority track's upper bound over `max`
            offset = TimestampOffset::min(
                offset,
                Timestamp::offset(priority.upper_bound.pts, now_pts + max),
            );
            if priority.lower_bound.stable {
                // but the priority track's own lower bound still gets at least `min` buffer, even
                // if that shifts its upper bound over `max`
                offset = TimestampOffset::max(
                    offset,
                    Timestamp::offset(priority.lower_bound.pts, now_pts + min),
                );
            }
        }

        offset
    }

    /// Whether the buffer `anchor` produces is within the range this strategy allows;
    /// when it is not, the anchor should be re-anchored at [`desired_anchor`](Self::desired_anchor).
    ///
    /// A lower bound that is not stable (see
    /// [`PtsBound::stable`](super::edge_estimator::PtsBound::stable)) is not judged.
    pub(super) fn buffer_in_range(
        &self,
        estimation: &EdgeEstimate,
        priority: Option<&EdgeEstimate>,
        anchor: TimestampOffset,
        now_pts: Timestamp,
    ) -> bool {
        let BufferingStrategy::Range { min, max, .. } = *self;

        // smallest offset: lower bound (the most pessimistic packets) with at least `min` buffer
        let mut min_offset = Timestamp::offset(estimation.lower_bound.pts, now_pts + min);
        if let Some(priority) = priority {
            // the other track may not shift the priority track's upper bound over `max`, so
            // we are ok with offset going lower than min if priority tracks would go over `max`
            min_offset = TimestampOffset::min(
                min_offset,
                Timestamp::offset(priority.upper_bound.pts, now_pts + max),
            );
            if priority.lower_bound.stable {
                // but the priority track's own lower bound still gets at least `min` buffer, even
                // if that shifts its upper bound over `max`
                min_offset = TimestampOffset::max(
                    min_offset,
                    Timestamp::offset(priority.lower_bound.pts, now_pts + min),
                );
            }
        }

        // largest offset: upper bound (the most optimistic packets) with at most `max` buffer,
        // unless the lower bound needs more
        let max_offset = TimestampOffset::max(
            Timestamp::offset(estimation.upper_bound.pts, now_pts + max),
            min_offset,
        );

        let min_ok = !estimation.lower_bound.stable || anchor >= min_offset;
        let max_ok = anchor <= max_offset;
        min_ok && max_ok
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
