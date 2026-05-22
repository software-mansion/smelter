//! PMT `stream_type` values from ISO/IEC 13818-1 Table 2-34.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    Mpeg2Video,
    Mpeg1Audio,
    Mpeg2Audio,
    AacAdts,
    AacLatm,
    H264,
    H265,
    Ac3,
    EAc3,
    Other(u8),
}

impl StreamType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0x02 => Self::Mpeg2Video,
            0x03 => Self::Mpeg1Audio,
            0x04 => Self::Mpeg2Audio,
            0x0F => Self::AacAdts,
            0x11 => Self::AacLatm,
            0x1B => Self::H264,
            0x24 => Self::H265,
            0x81 => Self::Ac3,
            0x87 => Self::EAc3,
            x => Self::Other(x),
        }
    }

    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Mpeg2Video => 0x02,
            Self::Mpeg1Audio => 0x03,
            Self::Mpeg2Audio => 0x04,
            Self::AacAdts => 0x0F,
            Self::AacLatm => 0x11,
            Self::H264 => 0x1B,
            Self::H265 => 0x24,
            Self::Ac3 => 0x81,
            Self::EAc3 => 0x87,
            Self::Other(x) => *x,
        }
    }

    pub fn is_video(&self) -> bool {
        matches!(self, Self::Mpeg2Video | Self::H264 | Self::H265)
    }

    pub fn is_audio(&self) -> bool {
        matches!(
            self,
            Self::Mpeg1Audio
                | Self::Mpeg2Audio
                | Self::AacAdts
                | Self::AacLatm
                | Self::Ac3
                | Self::EAc3
        )
    }
}
