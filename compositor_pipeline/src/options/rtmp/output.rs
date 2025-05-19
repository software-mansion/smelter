use std::sync::Arc;

use crate::*;

#[derive(Debug, Clone)]
pub struct RtmpOutputOptions {
    pub url: Arc<str>,
    pub video: Option<RtmpOutputVideoOptions>,
    pub audio: Option<RtmpOutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub struct RtmpOutputVideoOptions {
    pub encoder: VideoEncoderOptions,
    pub initial: VideoScene,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct RtmpOutputAudioOptions {
    pub encoder: RtmpAudioEncoderOptions,
    pub mixing_strategy: MixingStrategy,
    pub channels: AudioChannels,
    pub initial: AudioScene,
    pub end_condition: PipelineOutputEndCondition,
}


#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RtmpVideoEncoderOptions {
    H264(ffmpeg_h264::EncoderOptions),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RtmpAudioEncoderOptions {
    Aac(fdk_aac::EncoderOptions),
}
