use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use crate::pipeline::utils::input_sync::{InputSyncItem, TimestampAnchor};

use super::buffer::LiveSyncBuffer;

/// Flush signal of an input; every track observes each flush once.
#[derive(Default)]
pub(super) struct FlushState {
    generation: Arc<AtomicU64>,
}

impl FlushState {
    pub(super) fn flush(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn track_state(&self) -> TrackFlushState {
        TrackFlushState {
            handled: self.generation.load(Ordering::Relaxed),
            generation: self.generation.clone(),
        }
    }
}

pub(super) struct TrackFlushState {
    generation: Arc<AtomicU64>,
    handled: u64,
}

impl TrackFlushState {
    /// Signals a flush that this track's own
    /// [`should_flush`](Self::should_flush) does not report; concurrent
    /// flushes from other tracks stay pending.
    pub(super) fn flush(&mut self) {
        let previous = self.generation.fetch_add(1, Ordering::Relaxed);
        self.handled = previous + 1;
    }

    /// Returns `true` once per flush; flushes are not queued, multiple
    /// signals between calls collapse into one.
    pub(super) fn should_flush(&mut self) -> bool {
        let current = self.generation.load(Ordering::Relaxed);
        if current == self.handled {
            return false;
        }
        self.handled = current;
        true
    }
}

/// Buffers rotated out by a reset, each paired with the anchor it was mapped
/// with; drained oldest first, before anything buffered after the reset.
///
/// Tracks the output pts the queued content ends at, so a track that has to
/// flush before it ever established an edge can continue the output timeline
/// from there instead of jumping to the current playback position.
pub(super) struct FlushQueue<B: LiveSyncBuffer> {
    queue: VecDeque<(B, TimestampAnchor)>,
    end_pts: Option<Duration>,
}

impl<B: LiveSyncBuffer> Default for FlushQueue<B> {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            end_pts: None,
        }
    }
}

impl<B: LiveSyncBuffer> FlushQueue<B> {
    /// Queues a buffer mapped with `anchor`. `last_input_pts` is the newest
    /// pts observed on that timeline (`None` if nothing was observed); every
    /// item the buffer can release is at or below it.
    pub(super) fn push(
        &mut self,
        buffer: B,
        anchor: TimestampAnchor,
        last_input_pts: Option<Duration>,
    ) {
        if let Some(pts) = last_input_pts {
            let end_pts = anchor.to_output_pts(pts);
            self.end_pts = Some(match self.end_pts {
                Some(previous) => Duration::max(previous, end_pts),
                None => end_pts,
            });
        }
        self.queue.push_back((buffer, anchor));
    }

    /// Output pts the queued content ends at; `None` before the first flush.
    /// Stays available after everything drained.
    pub(super) fn end_pts(&self) -> Option<Duration> {
        self.end_pts
    }

    /// Next item with timestamps mapped onto the reference timeline; `None`
    /// when everything queued was already released.
    pub(super) fn read(&mut self) -> Option<B::Item> {
        while let Some((buffer, anchor)) = self.queue.front_mut() {
            if let Some(mut item) = buffer.read() {
                item.apply_anchor(*anchor);
                return Some(item);
            }
            self.queue.pop_front();
        }
        None
    }

    /// Output pts of the item [`read`](Self::read) would return.
    pub(super) fn peek_pts(&mut self) -> Option<Duration> {
        while let Some((buffer, anchor)) = self.queue.front() {
            if let Some(item) = buffer.peek() {
                return Some(anchor.to_output_pts(item.pts()));
            }
            self.queue.pop_front();
        }
        None
    }
}
