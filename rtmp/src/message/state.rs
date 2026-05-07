use std::collections::HashMap;

use crate::{AudioChannels, TrackId};

#[derive(Debug, Default, Clone, Copy, Hash, Eq, PartialEq)]
pub(crate) struct TrackKey {
    pub stream_id: u32,
    pub track_id: TrackId,
}

impl TrackKey {
    pub fn new(stream_id: u32, track_id: TrackId) -> Self {
        Self {
            stream_id,
            track_id,
        }
    }
}

/// Receiving side: tracks ack window to emit `Acknowledgement` messages.
#[derive(Debug, Default, Clone)]
pub(crate) struct ReceiverState {
    pub peer_window_ack_size: Option<u64>,
    pub bytes_at_last_ack: u64,
}
/// Sending side: per-track audio channel config keyed by `(stream_id, track_id)`.
/// Populated on `AudioConfig`, read on `AudioData` to fill legacy AAC frame headers.
#[derive(Debug, Default, Clone)]
pub(crate) struct SenderState {
    pub audio_channels: HashMap<TrackKey, AudioChannels>,
}
