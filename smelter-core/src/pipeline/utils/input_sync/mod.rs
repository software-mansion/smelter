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
//! Chunks leave a track through the callback passed to
//! [`InputSync::add_track`], with timestamps already mapped onto the output
//! timeline ([`InputSyncItem::apply_anchor`]).

use tracing::debug;

use crate::pipeline::decoder::DecoderThreadHandle;
use crate::prelude::*;

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

/// Receives the chunks a track releases, with timestamps already mapped onto
/// the output timeline. May be called while the input's sync lock is held,
/// so it must not call back into the sync; for live inputs it also runs on
/// network threads and must not block.
pub(crate) type TrackCallback<T> = Box<dyn FnMut(T) + Send>;

/// Decoder-channel convenience for [`InputSync`].
pub(crate) trait DecoderHandleInputSyncExt<B: LiveSyncBuffer> {
    /// Registers the track of the given kind; its chunks are forwarded to
    /// the decoder's channel wrapped in [`PipelineEvent::Data`]. The live
    /// variant must not block on a full channel, so chunks the channel
    /// cannot take are dropped; the non-live variant waits for room.
    fn add_track_with_handle(
        &self,
        kind: TrackKind,
        handle: DecoderThreadHandle,
    ) -> InputSyncTrack<B>;
}

impl<B: LiveSyncBuffer<Chunk = EncodedInputChunk>> DecoderHandleInputSyncExt<B> for InputSync<B> {
    fn add_track_with_handle(
        &self,
        kind: TrackKind,
        handle: DecoderThreadHandle,
    ) -> InputSyncTrack<B> {
        let is_live = matches!(self, InputSync::Live(_));
        self.add_track(
            kind,
            Box::new(move |chunk| {
                let event = PipelineEvent::Data(chunk);
                let result = match is_live {
                    true => handle.chunk_sender.try_send(event).map_err(|_| ()),
                    false => handle.chunk_sender.send(event).map_err(|_| ()),
                };
                if result.is_err() {
                    debug!("Dropping chunk; channel full or closed");
                }
            }),
        )
    }
}

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
    /// Registers the track of the given kind; `callback` receives its chunks
    /// once they are synchronized.
    pub fn add_track(
        &self,
        kind: TrackKind,
        callback: TrackCallback<B::Chunk>,
    ) -> InputSyncTrack<B> {
        match self {
            InputSync::Live(sync) => InputSyncTrack::Live(sync.add_track(kind, callback)),
            InputSync::Simple(sync) => InputSyncTrack::Simple(sync.add_track(callback)),
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
    Simple(SimpleSyncTrack<B::Chunk>),
}

impl<B: LiveSyncBuffer> InputSyncTrack<B> {
    pub fn write_chunk(&mut self, item: B::Chunk) {
        match self {
            InputSyncTrack::Live(track) => track.write_chunk(item),
            InputSyncTrack::Simple(track) => track.write_chunk(item),
        }
    }
}
