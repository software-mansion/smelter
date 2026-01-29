use thiserror::Error;

use crate::amf0::DecodingError;

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

    #[error("Error decoding amf0: {0}")]
    Amf0(DecodingError),

    #[error("AVC decoder config received more than once in one stream.")]
    AvcConfigDuplication,

    #[error("AAC decoder config received more than once in one stream.")]
    AacConfigDuplication,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum VideoTagParseError {
    #[error("Invalid AvcPacketType header value: {0}")]
    InvalidAvcPacketType(u8),

    #[error("Unsupported frame type header value: {0}")]
    UnsupportedFrameType(u8),
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AudioTagParseError {
    #[error("Invalid sound rate header value: {0}")]
    InvalidSoundRate(u8),

    #[error("Invalid sound type header value: {0}")]
    InvalidSoundType(u8),

    #[error("Invalid AacPacketType header value: {0}")]
    InvalidAacPacketType(u8),
}
