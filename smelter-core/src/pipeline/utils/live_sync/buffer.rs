use std::collections::VecDeque;

use crate::pipeline::utils::input_sync::InputSyncItem;
use crate::prelude::*;

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
}

/// Plain FIFO buffer: items come out in write order and nothing is ever held
/// back, so [`read`](LiveSyncBuffer::read) and
/// [`try_read`](LiveSyncBuffer::try_read) behave the same.
#[derive(Default)]
pub(crate) struct ChunkBuffer {
    queue: VecDeque<EncodedInputChunk>,
}

impl LiveSyncBuffer for ChunkBuffer {
    type Item = EncodedInputChunk;

    fn write(&mut self, item: EncodedInputChunk) {
        self.queue.push_back(item);
    }

    fn read(&mut self) -> Option<EncodedInputChunk> {
        self.queue.pop_front()
    }

    fn try_read(&mut self) -> Option<EncodedInputChunk> {
        self.queue.pop_front()
    }

    fn peek(&self) -> Option<&EncodedInputChunk> {
        self.queue.front()
    }
}
