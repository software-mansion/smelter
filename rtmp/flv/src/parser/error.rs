use thiserror::Error;

use crate::{AudioTagParseError, VideoTagParseError};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ParseError {
    #[error("Not enough data in FLV payload.")]
    NotEnoughData,

    #[error("Data is not a valid FLV header or tag header")]
    InvalidHeader,

    #[error("Unsupported codec header value: {0}")]
    UnsupportedCodec(u8),

    #[error("Filtered FLV packets are not supported.")]
    UnsupportedFiltered,

    #[error("Unsupported tag type: {0}")]
    UnsupportedTagType(u8),

    #[error("Error parsing audio tag: {0}")]
    Audio(AudioTagParseError),

    #[error("Error parsing video tag: {0}")]
    Video(VideoTagParseError),

    #[error("AVC decoder config received more than once in one stream.")]
    AvcConfigDuplication,

    #[error("AAC decoder config received more than once in one stream.")]
    AacConfigDuplication,
}
