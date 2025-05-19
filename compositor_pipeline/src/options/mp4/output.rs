use std::{path::Path, sync::Arc};

use crate::*;

#[derive(Debug, Clone)]
pub struct Mp4OutputOptions {
    pub output_path: Arc<Path>,
    pub video: Option<Mp4OutputVideoOptions>,
    pub audio: Option<Mp4OutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub struct Mp4OutputVideoOptions {
    pub encoder: VideoEncoderOptions,
    pub initial: VideoScene,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct Mp4OutputAudioOptions {
    pub encoder: Mp4AudioEncoderOptions,
    pub mixing_strategy: MixingStrategy,
    pub channels: AudioChannels,
    pub initial: AudioScene,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Mp4VideoEncoderOptions {
    H264(ffmpeg_h264::EncoderOptions),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Mp4AudioEncoderOptions {
    Aac(fdk_aac::EncoderOptions),
}
