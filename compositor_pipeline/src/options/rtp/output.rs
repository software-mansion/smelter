use std::sync::Arc;

use compositor_render::Resolution;

use crate::encoder::{AudioEncoderOptions, VideoEncoderOptions};
use crate::*;

#[derive(Debug, Clone)]
pub struct RtpOutputOptions {
    pub connection_options: RtpOutputConnectionOptions,
    pub video: Option<RtpOutputVideoOptions>,
    pub audio: Option<RtpOutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub struct RtpOutputVideoOptions {
    pub encoder: RtpVideoEncoderOptions,
    pub resolution: Resolution,
    pub initial: VideoScene,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct RtpOutputAudioOptions {
    pub mixing_strategy: MixingStrategy,
    pub channels: AudioChannels,
    pub encoder: RtpAudioEncoderOptions,
    pub initial: AudioScene,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpOutputConnectionOptions {
    Udp { port: Port, ip: Arc<str> },
    TcpServer { port: RequestedPort },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RtpVideoEncoderOptions {
    H264(ffmpeg_h264::EncoderOptions),
    VP8(ffmpeg_vp8::EncoderOptions),
    VP9(ffmpeg_vp9::EncoderOptions),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RtpAudioEncoderOptions {
    Opus(opus::EncoderOptions),
    Aac(fdk_aac::EncoderOptions),
}

impl From<RtpVideoEncoderOptions> for VideoEncoderOptions {
    fn from(value: RtpVideoEncoderOptions) -> Self {
        match value {
            RtpVideoEncoderOptions::H264(opt) => VideoEncoderOptions::H264(opt),
            RtpVideoEncoderOptions::VP8(opt) => VideoEncoderOptions::VP8(opt),
            RtpVideoEncoderOptions::VP9(opt) => VideoEncoderOptions::VP9(opt),
        }
    }
}

impl From<RtpAudioEncoderOptions> for AudioEncoderOptions {
    fn from(value: RtpAudioEncoderOptions) -> Self {
        match value {
            RtpAudioEncoderOptions::Opus(opts) => AudioEncoderOptions::Opus(opts),
            RtpAudioEncoderOptions::Aac(opts) => AudioEncoderOptions::Aac(opts),
        }
    }
}
