use thiserror::Error;

use crate::{
    AudioCodec, VideoCodec, VideoTagFrameType,
    amf3::{I29_MAX, I29_MIN, MAX_SEALED_COUNT, U28_MAX, U29_MAX},
};

#[derive(Error, Debug)]
pub enum RtmpError {
    #[error("Failed to establish RTMP connection.")]
    NegotiationFailed(#[from] RtmpNegotiationError),

    #[error("Failed to establish RTMP connection.")]
    ConnectionFailed(#[from] RtmpConnectionError),
}

#[derive(Error, Debug)]
pub enum RtmpNegotiationError {
    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),

    #[error(transparent)]
    Other(#[from] RtmpConnectionError),
}

#[derive(Error, Debug)]
pub enum RtmpConnectionError {
    #[error("IO error: {0}")]
    RtmpTcpSocket(#[from] std::io::Error),

    #[error("Failed to parse RTMP message stream: {0}")]
    MalformedRtmpStream(String),

    #[error("Received unknown RTMP message: {0}")]
    UnknownRtmpMessage(String),

    #[error("Error parsing audio tag: {0}")]
    FlvAudioParse(#[from] FlvAudioTagParseError),

    #[error("Error parsing video tag: {0}")]
    FlvVideoParse(#[from] FlvVideoTagParseError),

    #[error("Error parsing audio specific config: {0}")]
    AacConfigParse(#[from] AudioSpecificConfigParseError),

    #[error("Error decoding amf: {0}")]
    AmfDecoding(#[from] AmfDecodingError),
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
pub enum FlvVideoTagParseError {
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
pub enum FlvAudioTagParseError {
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
pub enum AudioSpecificConfigParseError {
    #[error("Invalid frequency index: {0}")]
    InvalidFrequencyIndex(u8),

    #[error("Invalid audio channel value in AAC audio specific config: {0}")]
    InvalidAudioChannel(u8),
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
