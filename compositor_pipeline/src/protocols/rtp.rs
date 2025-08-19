mod aac;

use std::{sync::Arc, time::Duration};

pub use aac::*;

use crate::{
    codecs::{
        AacAudioSpecificConfig, AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions,
    },
    protocols::{Port, PortOrRange},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpAudioOptions {
    Opus,
    FdkAac {
        asc: AacAudioSpecificConfig,
        raw_asc: bytes::Bytes,
        depayloader_mode: RtpAacDepayloaderMode,
    },
}

#[derive(Debug, Clone)]
pub struct RtpInputOptions {
    pub port: PortOrRange,
    pub transport_protocol: RtpInputTransportProtocol,
    pub video: Option<VideoDecoderOptions>,
    pub audio: Option<RtpAudioOptions>,

    /// Duration of stream that should be buffered before stream is started.
    /// If you have both audio and video streams then make sure to use the same value
    /// to avoid desync.
    ///
    /// This value defines minimal latency on the queue, but if you set it to low and fail
    /// to deliver the input stream on time it can cause either black screen or flickering image.
    ///
    /// By default DEFAULT_BUFFER_DURATION will be used.
    pub buffer_duration: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtpInputTransportProtocol {
    Udp,
    TcpServer,
}

#[derive(Debug, Clone)]
pub struct RtpOutputOptions {
    pub connection_options: RtpOutputConnectionOptions,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpOutputConnectionOptions {
    Udp { port: Port, ip: Arc<str> },
    TcpServer { port: PortOrRange },
}

impl RtpOutputConnectionOptions {
    pub fn mtu(&self) -> usize {
        match self {
            RtpOutputConnectionOptions::Udp { .. } => 1400,
            RtpOutputConnectionOptions::TcpServer { .. } => 64000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RtpInputError {
    #[error("Error while setting socket options.")]
    SocketOptions(#[source] std::io::Error),

    #[error("Error while binding the socket.")]
    SocketBind(#[source] std::io::Error),

    #[error("Failed to register input. Port: {0} is already used or not available.")]
    PortAlreadyInUse(u16),

    #[error("Failed to register input. All ports in range {lower_bound} to {upper_bound} are already used or not available.")]
    AllPortsAlreadyInUse { lower_bound: u16, upper_bound: u16 },
}
