use std::time::Duration;

use crate::prelude::*;

/// Item that can be buffered and synchronized by [`LiveSyncTrack`].
pub(crate) trait LiveSyncItem {
    /// Presentation timestamp in the input time base. Does not have to start
    /// at zero; [`LiveSyncStart::to_queue_pts`] maps produced timestamps onto
    /// the queue timeline.
    fn pts(&self) -> Duration;

    /// Whether decoding of this track can start from this item. Video should
    /// return true only for keyframes; audio should always return true.
    fn is_keyframe(&self) -> bool;
}

impl LiveSyncItem for EncodedInputChunk {
    fn pts(&self) -> Duration {
        todo!()
    }

    fn is_keyframe(&self) -> bool {
        todo!()
    }
}
