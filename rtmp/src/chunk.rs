#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ChunkType {
    Full = 0,
    NoMessageStreamId = 1,
    TimestampOnly = 2,
    NoHeader = 3,
}

impl From<u8> for ChunkType {
    fn from(v: u8) -> Self {
        match v {
            0 => ChunkType::Full,
            1 => ChunkType::NoMessageStreamId,
            2 => ChunkType::TimestampOnly,
            3 => ChunkType::NoHeader,
            _ => unreachable!("fmt field is only 2 bits"),
        }
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub(crate) struct ChunkHeader {
    pub fmt: ChunkType,
    pub cs_id: u32,
    pub timestamp: u32,
    pub timestamp_delta: u32,
    pub msg_len: u32,
    pub msg_type_id: u8,
    pub msg_stream_id: u32,
}
