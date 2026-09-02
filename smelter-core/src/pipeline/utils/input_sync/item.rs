use std::time::Duration;

use crate::prelude::*;

use super::TimestampAnchor;

/// Item that can be buffered and synchronized by an [`InputSyncTrack`].
///
/// [`InputSyncTrack`]: super::InputSyncTrack
pub(crate) trait InputSyncItem {
    /// Presentation timestamp in the input time base. Does not have to start
    /// at zero; timestamps are mapped onto the output timeline when the item
    /// is read from a track ([`InputSyncItem::apply_anchor`]).
    fn pts(&self) -> Duration;

    /// Size of the payload in bytes, for bitrate stats.
    fn size(&self) -> usize;

    /// Maps all timestamps of the item (pts, and dts if present) onto the
    /// output timeline `anchor` describes. Called by the track when the item
    /// is read.
    fn apply_anchor(&mut self, anchor: TimestampAnchor);

    /// Marks the item as decode-only: it still has to reach the decoder
    /// (later items need it decoded, e.g. video reference frames), but the
    /// content decoded from it must not be presented.
    fn mark_decode_only(&mut self);
}

impl InputSyncItem for EncodedInputChunk {
    fn pts(&self) -> Duration {
        self.pts
    }

    fn size(&self) -> usize {
        self.data.len()
    }

    fn apply_anchor(&mut self, anchor: TimestampAnchor) {
        self.pts = anchor.to_output_pts(self.pts);
        self.dts = self.dts.map(|dts| anchor.to_output_pts(dts));
    }

    fn mark_decode_only(&mut self) {
        self.present = false;
    }
}
