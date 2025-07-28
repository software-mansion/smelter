use core::fmt;
use std::time::Duration;

use crate::prelude::*;

pub struct EncodedInputChunk {
    pub data: bytes::Bytes,
    pub pts: Duration,
    pub dts: Option<Duration>,
    pub kind: MediaKind,
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
            .finish()
    }
}
