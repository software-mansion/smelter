use crate::*;

#[derive(Debug, Clone)]
pub struct RtpInputOptions {
    pub port: RequestedPort,
    pub transport_protocol: RtpTransportProtocol,
    pub video: Option<RtpInputVideoOptions>,
    pub audio: Option<RtpInputAudioOptions>,
    pub queue: queue::QueueInputOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtpInputVideoOptions {
    pub decoder: RtpVideoDecoder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpInputAudioOptions {
    Aac {
        decoder: fdk_aac::DecoderOptions,
        rtp_mode: RtpAacDepayloaderMode,
    },
    Opus {
        decoder: opus::DecoderOptions,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtpVideoDecoder {
    FfmpegH264,
    FfmpegVp8,
    FfmpegVp9,
    VulkanVideoH264,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtpTransportProtocol {
    Udp,
    TcpServer,
}

/// [RFC 3640, section 3.3.5. Low Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.5)
/// [RFC 3640, section 3.3.6. High Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.6)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtpAacDepayloaderMode {
    LowBitrate,
    HighBitrate,
}
