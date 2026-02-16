use std::sync::Arc;

use thiserror::Error;

use crate::{AudioCodec, VideoCodec, VideoTagFrameType, protocol::MessageType};

#[derive(Error, Debug)]
pub enum RtmpError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Handshake failed: {0}")]
    HandshakeFailed(Arc<str>),

    #[error("Message too large: {0} bytes")]
    MessageTooLarge(u32),

    #[error("Channel closed")]
    ChannelClosed,

    #[error("Unexpected EOF")]
    UnexpectedEof,

    #[error("Internal buffer error: {0}")]
    InternalBufferError(&'static str),

    #[error("Parsing error: {0}")]
    ParsingError(#[from] ParseError),

    #[error("Serialization error: {0}")]
    SerializeError(#[from] SerializationError),
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ParseError {
    #[error("Not enough data in FLV payload.")]
    NotEnoughData,

    #[error("Unknown RTMP message type: {0}")]
    UnknownMessageType(u8),

    #[error("Unsupported RTMP message type: {0:?}")]
    UnsupportedMessageType(MessageType),

    #[error("Error parsing audio tag: {0}")]
    Audio(#[from] AudioTagParseError),

    #[error("Error parsing video tag: {0}")]
    Video(#[from] VideoTagParseError),

    #[error("Error decoding amf0: {0}")]
    Amf0Decoding(#[from] AmfDecodingError),
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum SerializationError {
    #[error("Error encoding amf0: {0}")]
    Amf0Encoding(#[from] AmfEncodingError),

    #[error("Unsupported video codec: {0:?}")]
    UnsupportedVideoCodec(VideoCodec),

    #[error("Unsupported audio codec: {0:?}")]
    UnsupportedAudioCodec(AudioCodec),

    #[error("Packet type is required for AAC")]
    AacPacketTypeRequired,

    #[error("Packet type is required for H264")]
    H264PacketTypeRequired,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum VideoTagParseError {
    #[error("Invalid AvcPacketType header value: {0}")]
    InvalidAvcPacketType(u8),

    #[error("Unknown codec header value: {0}")]
    UnknownCodecId(u8),

    #[error("Unknown frame type header value: {0}")]
    UnknownFrameType(u8),

    #[error("Invalid frame type for H264 packet: {0:?}")]
    InvalidFrameTypeForH264(VideoTagFrameType),
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AudioTagParseError {
    #[error("Invalid sound rate header value: {0}")]
    InvalidSoundRate(u8),

    #[error("Invalid sound type header value: {0}")]
    InvalidSoundType(u8),

    #[error("Invalid AacPacketType header value: {0}")]
    InvalidAacPacketType(u8),

    #[error("Unknown codec header value: {0}")]
    UnknownCodecId(u8),
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AmfDecodingError {
    #[error("Unknown data type: {0}")]
    UnknownType(u8),

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
    Amf3(Amf3EncodingError),
}

const U29_MAX: u32 = (1 << 29) - 1;
const U28_MAX: u32 = (1 << 28) - 1;

const I29_MAX: i32 = (1 << 28) - 1;
const I29_MIN: i32 = -(1 << 28);

const MAX_SEALED_COUNT: u32 = (1 << 25) - 1;

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

impl From<Amf3EncodingError> for AmfEncodingError {
    fn from(value: Amf3EncodingError) -> Self {
        Self::Amf3(value)
    }
}
