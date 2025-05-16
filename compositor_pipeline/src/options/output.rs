use std::sync::Arc;

use compositor_render::{scene::Component, InputId};

use crate::*;

mod rtp;

#[derive(Debug, Clone)]
pub enum RegisterOutputOptions {
    Rtp(RtpOutputOptions),
    Rtmp(RtmpSenderOptions),
    Mp4(Mp4OutputOptions),
    Whip(WhipSenderOptions),
}

#[derive(Debug, Clone)]
pub struct RtpOutputOptions {
    pub connection_options: RtpConnectionOptions,
    pub video: Option<RtpOutputVideoOptions>,
    pub audio: Option<RtpOutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub struct RtpOutputVideoOptions {
    pub encoder: VideoEncoderOptions,
    pub initial: Component,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct RtpOutputAudioOptions {
    pub encoder: VideoEncoderOptions,
    pub initial: Component,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtpConnectionOptions {
    Udp { port: Port, ip: Arc<str> },
    TcpServer { port: RequestedPort },
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum VideoEncoderOptions {
    H264(ffmpeg_h264::Options),
    VP8(ffmpeg_vp8::Options),
    VP9(ffmpeg_vp9::Options),
}

#[derive(Debug, Clone)]
pub struct OutputVideoOptions {}

#[derive(Debug, Clone)]
pub struct OutputAudioOptions {
    pub initial: AudioMixingParams,
    pub mixing_strategy: MixingStrategy,
    pub channels: AudioChannels,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub enum PipelineOutputEndCondition {
    AnyOf(Vec<InputId>),
    AllOf(Vec<InputId>),
    AnyInput,
    AllInputs,
    Never,
}
