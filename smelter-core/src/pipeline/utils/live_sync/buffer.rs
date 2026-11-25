use std::collections::VecDeque;

use crate::{pipeline::utils::input_sync::InputSyncItem, stats::LiveSyncBufferStats};

use crate::prelude::*;

/// Storage for the chunks a [`LiveSyncTrack`] holds between write and release. It defines the
/// order chunks leave in: a plain FIFO keeps the write order, a jitter buffer can reorder
/// out-of-order delivery.
///
/// It never looks at the clock and never decides that something is late; the live sync looks
/// at the chunks from the front and takes them out one by one.
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

    /// Removes and returns the next item. Implementations that order items (e.g. a jitter
    /// buffer) give up on the ones that are missing before it.
    fn pop(&mut self) -> Option<Self::Chunk>;

    /// Items in the order [`pop`](Self::pop) returns them.
    fn iter(&self) -> impl Iterator<Item = &Self::Chunk>;

    /// Snapshot of the buffered content for stats.
    fn stats(&self) -> LiveSyncBufferStats;
}

/// Plain FIFO buffer: items come out in write order.
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

    fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    fn iter(&self) -> impl Iterator<Item = &T> {
        self.queue.iter()
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
