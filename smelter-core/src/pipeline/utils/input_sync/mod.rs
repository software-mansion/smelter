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

use tracing::debug;

use crate::pipeline::decoder::DecoderThreadHandle;
use crate::prelude::*;

use super::channel::{Sender, TrySendError};
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

/// Consumer of the chunks a track releases, with timestamps already mapped
/// onto the output timeline. Called while the input's sync lock is held, so
/// it must not call back into the sync; for live inputs it also runs on
/// network threads and must not block.
pub(crate) trait TrackSink<T>: Send {
    /// Pushes a chunk out. Anything that cannot be delivered is dropped.
    fn send(&mut self, item: T);

    /// Content sent after this call does not continue what came before: the
    /// input timeline was dropped, so state built from it (codec parameters,
    /// reference frames) does not describe the new one. Called between the
    /// last item of the old timeline and the first one of the new, and only
    /// on the track whose own timestamps broke.
    fn on_discontinuity(&mut self) {}

    /// Whether the consumer is gone. Once true it stays true.
    fn is_closed(&self) -> bool;
}

pub(crate) type BoxedTrackSink<T> = Box<dyn TrackSink<T>>;

/// The consumer of a track is gone, so nothing can be written to it anymore.
#[derive(Debug, thiserror::Error)]
#[error("Track is closed")]
pub(crate) struct TrackClosedError;

/// Decoder-channel convenience for [`InputSync`].
pub(crate) trait DecoderHandleInputSyncExt<B: LiveSyncBuffer> {
    /// Registers the track of the given kind; its chunks are forwarded to
    /// the decoder's channel wrapped in [`PipelineEvent::Data`]. A closed
    /// channel is reported by [`InputSyncTrack::write_chunk`].
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
            Box::new(DecoderTrackSink {
                chunk_sender: handle.chunk_sender,
                is_live,
                closed: false,
            }),
        )
    }
}

/// Forwards the chunks of a track to a decoder thread. The live variant must
/// not block on a full channel, so chunks the channel cannot take are
/// dropped; the non-live variant waits for room.
struct DecoderTrackSink {
    chunk_sender: Sender<PipelineEvent<EncodedInputChunk>>,
    is_live: bool,
    /// Set once the decoder side is gone. Only a send can observe it, so a
    /// track that never released anything does not notice.
    ///
    /// TODO: read it from the channel instead, so a track that is still
    /// buffering sees a closed decoder too. Needs `Sender::is_closed` in
    /// [`super::channel`], where `receiver_alive` already tracks it.
    closed: bool,
}

impl TrackSink<EncodedInputChunk> for DecoderTrackSink {
    fn send(&mut self, chunk: EncodedInputChunk) {
        let event = PipelineEvent::Data(chunk);
        match self.is_live {
            true => match self.chunk_sender.try_send(event) {
                Ok(()) => (),
                Err(TrySendError::Full(_)) => debug!("Dropping chunk; decoder is not keeping up"),
                Err(TrySendError::Disconnected(_)) => self.closed = true,
            },
            false => {
                if self.chunk_sender.send(event).is_err() {
                    self.closed = true;
                }
            }
        }
    }

    // TODO: implement `on_discontinuity` once the decoder channel carries
    // `EncodedInputEvent` instead of `EncodedInputChunk`: send
    // `EncodedInputEvent::Discontinuity`, so the bytestream transformer
    // re-emits the parameter sets (`H264AvccToAnnexB` keeps the config it
    // needs for that) and the decoder drops the state built for the old
    // timeline. The marker has to survive a full channel, so it needs to be
    // held and sent ahead of the next chunk the channel accepts instead of
    // being dropped like one; a lost marker leaves the decoder decoding the
    // new timeline with the old state.
    //
    // A source that comes back with a different codec config needs the
    // decoder thread respawned rather than reset, and this sink is where
    // that decision belongs - the timestamps live sync sees cannot tell the
    // two apart.

    fn is_closed(&self) -> bool {
        self.closed
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
    /// Registers the track of the given kind; `sink` receives its chunks
    /// once they are synchronized.
    pub fn add_track(&self, kind: TrackKind, sink: BoxedTrackSink<B::Chunk>) -> InputSyncTrack<B> {
        match self {
            InputSync::Live(sync) => InputSyncTrack::Live(sync.add_track(kind, sink)),
            InputSync::Simple(sync) => InputSyncTrack::Simple(sync.add_track(sink)),
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
    /// Writes a chunk to the track. Fails once the consumer of the track is
    /// gone; the caller is expected to stop producing for this input.
    pub fn write_chunk(&mut self, item: B::Chunk) -> Result<(), TrackClosedError> {
        match self {
            InputSyncTrack::Live(track) => track.write_chunk(item),
            InputSyncTrack::Simple(track) => track.write_chunk(item),
        }
    }
}
