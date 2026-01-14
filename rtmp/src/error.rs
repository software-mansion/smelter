use std::sync::Arc;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RtmpError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid RTMP version: {0}")]
    InvalidVersion(u8),

    #[error("Handshake failed: {0}")]
    HandshakeFailed(Arc<str>),

    #[error("Message too large: {0} bytes")]
    MessageTooLarge(u32),

    #[error("Unsupported RTMP message type: {0}")]
    UnsuportedMessageType(u8),

    #[error("Connection timeout")]
    Timeout,

    #[error("Stream not registered")]
    StreamNotRegistered,

    #[error("Socket closed")]
    SocketClosed,

    #[error("Missing previous chunk header for CSID {0}")]
    MissingHeader(u32),

    #[error("Unexpected EOF")]
    UnexpectedEof,

    #[error("Would Block")]
    WouldBlock,

    #[error("Internal buffer error: {0}")]
    InternalBufferError(&'static str),
}
