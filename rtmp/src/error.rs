use thiserror::Error;

use crate::VideoTagFrameType;

#[derive(Error, Debug)]
pub enum RtmpConnectionError {
    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("Server returned _error in response to connect: {0:?}")]
    ErrorOnConnect(String),

    #[error("Server returned _error in response to createStream: {0:?}")]
    ErrorOnCreateStream(String),

    #[error("Failed to establish TCP connection")]
    TcpSocket(#[from] std::io::Error),

    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),

    #[error("Invalid DNS name: {0}")]
    InvalidDnsName(#[from] rustls::pki_types::InvalidDnsNameError),

    #[error("TLS configuration error: {0}")]
    TlsConfig(String),

    #[error(transparent)]
    StreamError(#[from] RtmpStreamError),
}

impl RtmpConnectionError {
    /// If error is critical connection should be aborted
    pub fn is_critical(&self) -> bool {
        match self {
            Self::HandshakeFailed(_) => true,
            Self::TcpSocket(_) => true,
            Self::Tls(_) => true,
            Self::InvalidDnsName(_) => true,
            Self::TlsConfig(_) => true,
            Self::StreamError(err) => err.is_critical(),
            _ => true,
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

    #[error("Failed to parse command message")]
    CommandMessage(#[from] CommandMessageParseError),

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

#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum CommandMessageParseError {
    #[error(transparent)]
    Amf(#[from] AmfDecodingError),

    #[error("Missing command name")]
    MissingCommandName,

    #[error("Missing transaction_id")]
    MissingTransactionId,

    #[error("Unexpected AMF value type for field: {field}")]
    UnexpectedValueType { field: &'static str },
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

    #[error("Unknown VideoFourCc: {0:?}")]
    UnknownVideoFourCc([u8; 4]),

    #[error("Unknown ExVideoPacketType header value: {0}")]
    UnknownExVideoPacketType(u8),

    #[error("Unknown VideoPacketModExType header value: {0}")]
    UnknownVideoPacketModExType(u8),

    #[error("Unsupported video packet type: {0}")]
    UnsupportedPacketType(u8),

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

    #[error("Insufficient data")]
    InsufficientData,

    #[error("Invalid UTF-8 string")]
    InvalidUtf8,

    #[error("Complex type reference out of bounds")]
    OutOfBoundsReference,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum AmfEncodingError {
    #[error("String too long: {0} bytes (max {}).", u16::MAX)]
    StringTooLong(usize),

    #[error("Array too long: {0} elements (max {}).", u32::MAX)]
    ArrayTooLong(usize),

    #[error("Long string too long: {0} bytes (max {}).", u32::MAX)]
    LongStringTooLong(usize),
}
