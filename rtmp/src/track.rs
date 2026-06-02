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
