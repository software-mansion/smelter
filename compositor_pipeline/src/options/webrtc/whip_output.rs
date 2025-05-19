use std::sync::Arc;

use crate::*;

#[derive(Debug, Clone)]
pub struct WhipOutputOptions {
    pub endpoint_url: Arc<str>,
    pub bearer_token: Option<Arc<str>>,
    pub video: Option<WhipOutputVideoOptions>,
    pub audio: Option<WhipOutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub struct WhipOutputVideoOptions {
    pub encoder_preferences: Vec<WhipVideoEncoderOptions>,
    pub initial: VideoScene,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct WhipOutputAudioOptions {
    pub encoder_preferences: Vec<WhipAudioEncoderOptions>,
    pub mixing_strategy: MixingStrategy,
    pub channels: AudioChannels,
    pub initial: AudioScene,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WhipVideoEncoderOptions {
    H264(ffmpeg_h264::EncoderOptions),
    VP8(ffmpeg_vp8::EncoderOptions),
    VP9(ffmpeg_vp9::EncoderOptions),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WhipAudioEncoderOptions {
    Opus(opus::EncoderOptions),
}
