/// Identifier for a logical track within an RTMP stream.
///
/// For single-track streams (present case) [`TrackId::PRIMARY`] is used. When
/// Enhanced RTMP multitrack parsing lands, non-primary ids will be populated
/// from the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackId(pub u8);

impl TrackId {
    pub const PRIMARY: Self = Self(0);
}

impl Default for TrackId {
    fn default() -> Self {
        Self::PRIMARY
    }
}

/// Session-level addressing for a single track inside a single RTMP stream.
///
/// `(stream_id, track_id)` is the natural key for any per-track state that a
/// connection has to keep across messages — e.g. the AAC channel layout that
/// `AudioConfig` declares once and every legacy `AudioData` frame must replay.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub(crate) struct TrackKey {
    pub stream_id: u32,
    pub track_id: TrackId,
}

impl TrackKey {
    pub fn new(stream_id: u32, track_id: TrackId) -> Self {
        Self { stream_id, track_id }
    }
}
