use crate::prelude::*;

/// Item that can be buffered and synchronized by an [`InputSyncTrack`].
///
/// [`InputSyncTrack`]: super::InputSyncTrack
pub(crate) trait InputSyncItem {
    /// Presentation timestamp in the input time base. Does not have to start
    /// at zero; timestamps are mapped onto the output timeline when the item
    /// is read from a track ([`InputSyncItem::apply_anchor`]).
    fn pts(&self) -> Timestamp;

    /// Size of the payload in bytes, for bitrate stats.
    fn size(&self) -> usize;

    /// Whether the item can be decoded without the items before it. Always true for audio.
    fn is_keyframe(&self) -> bool;

    /// Shifts all timestamps of the item (pts, and dts if present) by `anchor`. Shifts add up:
    /// the live sync applies one when the item is written (a stitched discontinuity) and one
    /// when it is read (the mapping onto the output timeline).
    fn apply_anchor(&mut self, anchor: TimestampOffset);

    /// Marks the item as decode-only: it still has to reach the decoder
    /// (later items need it decoded, e.g. video reference frames), but the
    /// content decoded from it must not be presented.
    fn mark_decode_only(&mut self);
}
