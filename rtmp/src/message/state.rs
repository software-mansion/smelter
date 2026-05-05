use std::collections::HashMap;

use crate::{AudioChannels, TrackId};

/// Per-direction stream state owned by the message-stream layer.
///
/// Two scopes:
/// - `tracks`: keyed by `(message_stream_id, track_id)`. Holds anything that
///   varies per logical track (codec config, channel layout, last DTS, ...).
/// - `session`: connection-wide state (chunk size, ack window, negotiated
///   capabilities, last `onMetaData`, ...).
#[derive(Debug, Default, Clone)]
pub(crate) struct RtmpStreamState {
    tracks: HashMap<TrackKey, TrackState>,
    session: SessionState,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct SessionState {
    // Reserved for connection-wide state: peer chunk size, negotiated capsEx,
    // fourCcList caps, cached onMetaData, ...
}

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

#[derive(Debug, Default, Clone)]
pub(crate) struct TrackState {
    pub audio: Option<AudioTrackState>,
    // future: pub video: Option<VideoTrackState>,
}

/// Internal audio bookkeeping. Populated on Config-class messages, consulted
/// on Data-class messages to fill wire fields the codec frame omits (e.g. the
/// legacy AAC SoundType bit).
#[derive(Debug, Clone, Copy)]
pub(crate) struct AudioTrackState {
    pub channels: AudioChannels,
}

#[allow(dead_code)]
impl RtmpStreamState {
    pub fn track(&self, key: TrackKey) -> Option<&TrackState> {
        self.tracks.get(&key)
    }

    pub fn track_mut(&mut self, key: TrackKey) -> &mut TrackState {
        self.tracks.entry(key).or_default()
    }

    pub fn session(&self) -> &SessionState {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut SessionState {
        &mut self.session
    }
}
