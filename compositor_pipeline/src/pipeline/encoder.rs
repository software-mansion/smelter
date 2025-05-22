use std::sync::Arc;

use compositor_render::{Frame, OutputId, Resolution};
use crossbeam_channel::{bounded, Receiver, Sender};
use log::error;
use resampler::OutputResampler;

use crate::{
    audio_mixer::{AudioChannels, OutputSamples},
    error::EncoderInitError,
    queue::PipelineEvent,
};

use self::opus::OpusEncoder;

use super::{types::EncoderOutputEvent, EncodedChunk, PipelineCtx};

pub(crate) mod audio_encoder_thread;
pub(crate) mod video_encoder_thread;

pub mod fdk_aac;
pub mod ffmpeg_h264;
pub mod ffmpeg_vp8;
pub mod ffmpeg_vp9;
pub mod opus;
mod resampler;

pub struct EncoderOptions {
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum VideoEncoderOptions {
    H264(ffmpeg_h264::Options),
    VP8(ffmpeg_vp8::Options),
    VP9(ffmpeg_vp9::Options),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum AudioEncoderOptions {
    Opus(opus::OpusEncoderOptions),
    Aac(fdk_aac::AacEncoderOptions),
}

pub struct EncoderContext {
    pub video: Option<VideoEncoderContext>,
    pub audio: Option<AudioEncoderContext>,
}

#[derive(Debug, Clone)]
pub enum VideoEncoderContext {
    H264(Option<bytes::Bytes>),
    VP8,
    VP9,
}

#[derive(Debug, Clone)]
pub enum AudioEncoderContext {
    Opus,
    Aac(bytes::Bytes),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum AudioEncoderPreset {
    Quality,
    Voip,
    LowestLatency,
}

impl VideoEncoderOptions {
    pub fn resolution(&self) -> Resolution {
        match self {
            VideoEncoderOptions::H264(opt) => opt.resolution,
            VideoEncoderOptions::VP8(opt) => opt.resolution,
            VideoEncoderOptions::VP9(opt) => opt.resolution,
        }
    }
}

impl AudioEncoderOptions {
    pub fn channels(&self) -> AudioChannels {
        match self {
            AudioEncoderOptions::Opus(options) => options.channels,
            AudioEncoderOptions::Aac(options) => options.channels,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        match self {
            AudioEncoderOptions::Opus(options) => options.sample_rate,
            AudioEncoderOptions::Aac(options) => options.sample_rate,
        }
    }
}

struct VideoEncoderConfig {
    resolution: Resolution,
    extradata: Option<bytes::Bytes>,
}

trait VideoEncoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError>;
    fn encode(&mut self, frame: Frame) -> Vec<EncodedChunk>;
    fn flush(&mut self) -> Vec<EncodedChunk>;
    fn request_keyframe(&mut self);
}

struct AudioEncoderConfig {
    channels: AudioChannels,
    sample_rate: u32,
    extradata: Option<bytes::Bytes>,
}

trait AudioEncoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, AudioEncoderConfig), EncoderInitError>;
    fn encode(&mut self, samples: OutputSamples) -> Vec<EncodedChunk>;
    fn flush(&mut self) -> Vec<EncodedChunk>;
}
