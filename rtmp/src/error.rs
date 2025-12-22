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

    #[error("Connection timeout")]
    Timeout,

    #[error("Stream not registered")]
    StreamNotRegistered,
}
