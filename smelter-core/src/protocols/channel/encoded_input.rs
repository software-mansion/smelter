use core::fmt;
use std::time::Duration;

use crate::prelude::*;

pub struct EncodedInputChunk {
    pub data: bytes::Bytes,
    pub pts: Duration,
    pub dts: Option<Duration>,
    pub kind: MediaKind,

    /// Sometimes we need to send data to the decoder, so the next chunks can
    /// be decoded correctly, but resulting frames should not be sent to the queue.
    /// In those cases this field should be set to false.
    pub present: bool,
}

impl fmt::Debug for EncodedInputChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.data.len();
        let first_bytes = &self.data[0..usize::min(10, len)];
        f.debug_struct("EncodedChunk")
            .field("data", &format!("len={len}, {first_bytes:?}"))
            .field("pts", &self.pts)
            .field("dts", &self.dts)
            .field("kind", &self.kind)
            .field("present", &self.present)
            .finish()
    }
}
