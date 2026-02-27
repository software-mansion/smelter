use thiserror::Error;

use crate::{
    VideoTagFrameType,
    amf3::{I29_MAX, I29_MIN, MAX_SEALED_COUNT, U28_MAX, U29_MAX},
};

#[derive(Error, Debug)]
pub enum RtmpConnectionError {
    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("Failed to establish TCP connection")]
    TcpSocket(#[from] std::io::Error),

    #[error(transparent)]
    StreamError(#[from] RtmpStreamError),
}

impl RtmpConnectionError {
    /// If error is critical connection should be aborted
    pub fn is_critical(&self) -> bool {
        match self {
            Self::HandshakeFailed(_) => true,
            Self::TcpSocket(_) => true,
            Self::StreamError(err) => err.is_critical(),
        }
    }
}

#[derive(Error, Debug)]
pub enum RtmpStreamError {
    #[error("IO error: {0}")]
    TcpError(#[from] std::io::Error),

    #[error("Failed to parse RTMP message stream: {0}")]
    ReceivedMalformedStream(String),

    #[error("Received unknown RTMP message")]
    ParseMessage(#[from] RtmpMessageParseError),

    #[error(transparent)]
    SerializeMessage(#[from] RtmpMessageSerializeError),
}

impl RtmpStreamError {
    /// If error is critical connection should be aborted
    pub fn is_critical(&self) -> bool {
        match self {
            Self::TcpError(_) => true,
            Self::ReceivedMalformedStream(_) => true,
            Self::ParseMessage(_) => false,
            Self::SerializeMessage(_) => false,
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum RtmpMessageSerializeError {
    #[error("Error encoding amf0: {0}")]
    Amf0Encoding(#[from] AmfEncodingError),

    #[error("Failed to serialize message: {0}")]
    InternalError(String),
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum RtmpMessageParseError {
    #[error("Invalid message type: {0}")]
    InvalidMessageType(u8),

    #[error("Received unsupported message: {0}")]
    UnsupportedMessage(String),

    #[error("Unknown UserControlMessageKind {0}")]
    InvalidUserControlMessage(u16),

    #[error("Error parsing audio tag")]
    FlvAudioParse(#[from] FlvAudioTagParseError),

    #[error("Error parsing video tag")]
    FlvVideoParse(#[from] FlvVideoTagParseError),

    #[error("Error parsing audio specific config")]
    AacConfigParse(#[from] AacConfigParseError),

    #[error("Error decoding AMF value")]
    AmfDecoding(#[from] AmfDecodingError),

    #[error("Message payload too short")]
    PayloadTooShort,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum FlvVideoTagParseError {
    #[error("Invalid AvcPacketType header value: {0}")]
    InvalidAvcPacketType(u8),

    #[error("Unknown codec header value: {0}")]
    UnknownCodecId(u8),

    #[error("Unknown frame type header value: {0}")]
    UnknownFrameType(u8),

    #[error("Invalid frame type for H264 packet: {0:?}")]
    InvalidFrameTypeForH264(VideoTagFrameType),

    #[error("Invalid video tag, packet too short.")]
    TooShort,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum FlvAudioTagParseError {
    #[error("Invalid sound rate header value: {0}")]
    InvalidSoundRate(u8),

    #[error("Invalid sound type header value: {0}")]
    InvalidSoundType(u8),

    #[error("Invalid AacPacketType header value: {0}")]
    InvalidAacPacketType(u8),

    #[error("Unknown codec header value: {0}")]
    UnknownCodecId(u8),

    #[error("Invalid audio tag, packet too short.")]
    TooShort,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AacConfigParseError {
    #[error("Invalid frequency index: {0}")]
    InvalidFrequencyIndex(u8),

    #[error("Invalid audio channel value in AAC audio specific config: {0}")]
    InvalidAudioChannel(u8),

    #[error("Not enough data, config too short")]
    TooShort,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AmfDecodingError {
    #[error("Unknown data type: {0}")]
    UnknownType(u8),

    #[error("Format selector must always be 0.")]
    InvalidFormatSelector,

    #[error("Insufficient data")]
    InsufficientData,

    #[error("Invalid UTF-8 string")]
    InvalidUtf8,

    #[error("Complex type reference out of bounds")]
    OutOfBoundsReference,

    #[error("Reference points to object of different amf type than expected.")]
    InvalidReferenceType,

    #[error("Handling of externalizable object traits is not implemented.")]
    ExternalizableTrait,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AmfEncodingError {
    #[error("String too long: {0} bytes (max {}).", u16::MAX)]
    StringTooLong(usize),

    #[error("Array too long: {0} elements (max {}).", u32::MAX)]
    ArrayTooLong(usize),

    #[error("Long string too long: {0} bytes (max {}).", u32::MAX)]
    LongStringTooLong(usize),

    #[error("AMF3 encoding error: {0}.")]
    Amf3(#[from] Amf3EncodingError),
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum Amf3EncodingError {
    #[error("String too long: {0} bytes (max {U28_MAX}).")]
    StringTooLong(usize),

    #[error("Array too long: {0} elements (max {U28_MAX}).")]
    ArrayTooLong(usize),

    #[error("Vector too long: {0} elements (max {U28_MAX}).")]
    VectorTooLong(usize),

    #[error(
        "Sealed count larger than actual number of object members. (Sealed count: {sealed_count}, Actual members: {actual_members})."
    )]
    SealedCountTooLarge {
        sealed_count: usize,
        actual_members: usize,
    },

    #[error("Too many sealed members in an object: {0} elements (max {MAX_SEALED_COUNT}).")]
    SealedMembersCountTooLarge(usize),

    #[error("Dictionary too long: {0} entries (max {U28_MAX}).")]
    DictionaryTooLong(usize),

    #[error("Integer must be in range [{I29_MIN}, {I29_MAX}].")]
    OutOfRangeInteger,

    #[error("U29 must be in range [0, {U29_MAX}].")]
    OutOfRangeU29,
}
