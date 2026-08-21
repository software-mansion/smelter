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
//! Chunks leave a track through the sink passed to
//! [`InputSync::add_track`], with timestamps already mapped onto the output
//! timeline ([`InputSyncItem::apply_anchor`]).

use super::live_sync::{LiveSync, LiveSyncBuffer, LiveSyncTrack};

mod anchor;
mod item;
mod simple_sync;

pub(crate) use anchor::TimestampAnchor;
pub(crate) use item::InputSyncItem;
pub(crate) use simple_sync::{SimpleSync, SimpleSyncTrack};

/// Kind of a track registered on an input sync; an input has at most one
/// track of each kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackKind {
    Audio,
    Video,
}

/// What a track releases.
pub(crate) enum TrackEvent<T> {
    /// Chunk with timestamps already mapped onto the output timeline.
    Chunk(T),
    /// Chunks after this one do not continue what came before: the input
    /// timeline was dropped, so state built from it (codec parameters,
    /// reference frames) does not describe the new one. Sent between the last
    /// chunk of the old timeline and the first one of the new, and only on
    /// the track whose own timestamps broke.
    Discontinuity,
}

/// Consumer of what a track releases. Called while the input's sync lock is
/// held, so it must not call back into the sync; for live inputs it also runs
/// on network threads and must not block.
pub(crate) trait TrackSink<T>: Send {
    /// Handles a released event. Anything that cannot be delivered is dropped.
    fn on_event(&mut self, event: TrackEvent<T>);

    /// Whether the consumer is gone. Once true it stays true.
    fn is_closed(&self) -> bool;
}

pub(crate) type BoxedTrackSink<T> = Box<dyn TrackSink<T>>;

/// The consumer of a track is gone, so nothing can be written to it anymore.
#[derive(Debug, thiserror::Error)]
#[error("Track is closed")]
pub(crate) struct TrackClosedError;

/// Synchronization of a single input; register per-track callbacks with
/// [`InputSync::add_track`]. The buffer type decides how the live variant
/// buffers chunks (e.g. [`ChunkBuffer`] for in-order delivery).
///
/// [`ChunkBuffer`]: super::live_sync::ChunkBuffer
pub(crate) enum InputSync<B: LiveSyncBuffer> {
    Live(LiveSync<B>),
    Simple(SimpleSync),
}

impl<B: LiveSyncBuffer> InputSync<B> {
    /// Registers the track of the given kind; `sink` receives its chunks
    /// once they are synchronized.
    pub fn add_track(&self, kind: TrackKind, sink: BoxedTrackSink<B::Chunk>) -> InputSyncTrack<B> {
        match self {
            InputSync::Live(sync) => InputSyncTrack::Live(sync.add_track(kind, sink)),
            InputSync::Simple(sync) => InputSyncTrack::Simple(sync.add_track(sink)),
        }
    }

    /// Whether chunks are synchronized to a live edge, i.e. held back until
    /// it is estimated and dropped when a consumer cannot keep up.
    pub fn is_live(&self) -> bool {
        matches!(self, InputSync::Live(_))
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
    Simple(SimpleSyncTrack<B::Chunk>),
}

impl<B: LiveSyncBuffer> InputSyncTrack<B> {
    /// Writes a chunk to the track. Fails once the consumer of the track is
    /// gone; the caller is expected to stop producing for this input.
    pub fn write_chunk(&mut self, item: B::Chunk) -> Result<(), TrackClosedError> {
        match self {
            InputSyncTrack::Live(track) => track.write_chunk(item),
            InputSyncTrack::Simple(track) => track.write_chunk(item),
        }
    }
}
