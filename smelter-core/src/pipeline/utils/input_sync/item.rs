use std::time::Duration;

use crate::prelude::*;

/// Item that can be buffered and synchronized by an [`InputSyncTrack`].
///
/// [`InputSyncTrack`]: super::InputSyncTrack
pub(crate) trait InputSyncItem {
    /// Presentation timestamp in the input time base. Does not have to start
    /// at zero; timestamps are mapped onto the output timeline when the item
    /// is read from a track ([`InputSyncItem::map_timestamps`]).
    fn pts(&self) -> Duration;

    /// Applies `map` to all timestamps of the item (pts, and dts if present).
    /// Called by the track when the item is read.
    fn map_timestamps(&mut self, map: impl Fn(Duration) -> Duration);
}

impl InputSyncItem for EncodedInputChunk {
    fn pts(&self) -> Duration {
        self.pts
    }

    fn map_timestamps(&mut self, map: impl Fn(Duration) -> Duration) {
        self.pts = map(self.pts);
        self.dts = self.dts.map(map);
    }
}
