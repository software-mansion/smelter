//! Common interface over track synchronization strategies for inputs that
//! support both live and non-live streams.
//!
//! Each variant defines its own output timeline, so the queue track has to be
//! registered to match the variant used:
//! - [`InputSync::Live`] ([`LiveSync`]) maps timestamps onto the timeline of
//!   the queue sync point; register with `QueueTrackOffset::Pts(Duration::ZERO)`.
//! - [`InputSync::Simple`] ([`SimpleSync`]) normalizes timestamps to start at
//!   zero; register with `QueueTrackOffset::None` so the queue fixes the
//!   placement on the first received packet.
//!
//! Chunks read from a track already have their timestamps mapped onto the
//! output timeline ([`InputSyncItem::apply_anchor`]).

use std::time::Duration;

use super::live_sync::{LiveSync, LiveSyncBuffer, LiveSyncTrack};

mod anchor;
mod item;
mod simple_sync;

pub(crate) use anchor::TimestampAnchor;
pub(crate) use item::InputSyncItem;
pub(crate) use simple_sync::{SimpleSync, SimpleSyncTrack};

/// Synchronization of a single input; create per-track handles with
/// [`InputSync::add_track`].
pub(crate) enum InputSync {
    Live(LiveSync),
    Simple(SimpleSync),
}

impl InputSync {
    /// Registers a new track; the buffer type decides how the live variant
    /// buffers chunks (e.g. [`ChunkBuffer`] for in-order delivery).
    ///
    /// [`ChunkBuffer`]: super::live_sync::ChunkBuffer
    pub fn add_track<B: LiveSyncBuffer>(&self) -> InputSyncTrack<B> {
        match self {
            InputSync::Live(sync) => InputSyncTrack::Live(sync.add_track()),
            InputSync::Simple(sync) => InputSyncTrack::Simple(sync.add_track()),
        }
    }

    /// Give up on any pending detection and release everything that is
    /// buffered (e.g. when the stream ended).
    pub fn flush(&self) {
        match self {
            InputSync::Live(sync) => sync.flush(),
            // SimpleSync never holds chunks back
            InputSync::Simple(_) => (),
        }
    }
}

pub(crate) enum InputSyncTrack<B: LiveSyncBuffer> {
    Live(LiveSyncTrack<B>),
    Simple(SimpleSyncTrack<B::Item>),
}

impl<B: LiveSyncBuffer> InputSyncTrack<B> {
    pub fn write_chunk(&mut self, item: B::Item) {
        match self {
            InputSyncTrack::Live(track) => track.write_chunk(item),
            InputSyncTrack::Simple(track) => track.write_chunk(item),
        }
    }

    /// Returns buffered chunks in write order with timestamps mapped onto the
    /// output timeline; `None` when no chunk can be produced right now.
    pub fn try_read_chunk(&mut self) -> Option<B::Item> {
        match self {
            InputSyncTrack::Live(track) => track.try_read_chunk(),
            InputSyncTrack::Simple(track) => track.try_read_chunk(),
        }
    }

    /// Raw pts of the next readable chunk; enables interleaved reads across
    /// tracks.
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        match self {
            InputSyncTrack::Live(track) => track.peek_next_pts(),
            InputSyncTrack::Simple(track) => track.peek_next_pts(),
        }
    }
}
