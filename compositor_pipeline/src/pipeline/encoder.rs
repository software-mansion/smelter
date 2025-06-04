use std::sync::Arc;

use compositor_render::{Frame, OutputFrameFormat, OutputId, Resolution};
use crossbeam_channel::{bounded, Receiver, Sender};
use fdk_aac::AacEncoder;
use ffmpeg_next::format::Pixel;
use ffmpeg_vp8::LibavVP8Encoder;
use ffmpeg_vp9::LibavVP9Encoder;
use log::error;
use resampler::OutputResampler;

use crate::{
    audio_mixer::{AudioChannels, OutputSamples},
    error::EncoderInitError,
    queue::PipelineEvent,
};

use self::{ffmpeg_h264::LibavH264Encoder, opus::OpusEncoder};

use super::{types::EncoderOutputEvent, PipelineCtx};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputPixelFormat {
    YUV420P,
    YUV422P,
    YUV444P,
}

impl From<OutputPixelFormat> for Pixel {
    fn from(format: OutputPixelFormat) -> Self {
        match format {
            OutputPixelFormat::YUV420P => Pixel::YUV420P,
            OutputPixelFormat::YUV422P => Pixel::YUV422P,
            OutputPixelFormat::YUV444P => Pixel::YUV444P,
        }
    }
}

impl From<OutputPixelFormat> for OutputFrameFormat {
    fn from(format: OutputPixelFormat) -> Self {
        match format {
            OutputPixelFormat::YUV420P => OutputFrameFormat::PlanarYuv420Bytes,
            OutputPixelFormat::YUV422P => OutputFrameFormat::PlanarYuv422Bytes,
            OutputPixelFormat::YUV444P => OutputFrameFormat::PlanarYuv444Bytes,
        }
    }
}

pub struct Encoder {
    pub video: Option<VideoEncoder>,
    pub audio: Option<AudioEncoder>,
}

pub enum VideoEncoder {
    H264(LibavH264Encoder),
    VP8(LibavVP8Encoder),
    VP9(LibavVP9Encoder),
}

pub enum AudioEncoder {
    Opus(OpusEncoder),
    Aac(AacEncoder),
}

impl Encoder {
    pub fn new(
        output_id: &OutputId,
        options: EncoderOptions,
        ctx: &Arc<PipelineCtx>,
    ) -> Result<(Self, Receiver<EncoderOutputEvent>), EncoderInitError> {
        let (encoded_chunks_sender, encoded_chunks_receiver) = bounded(1);

        let video_encoder = match options.video {
            Some(video_encoder_options) => Some(VideoEncoder::new(
                output_id,
                video_encoder_options,
                ctx,
                encoded_chunks_sender.clone(),
            )?),
            None => None,
        };

        let audio_encoder = match options.audio {
            Some(audio_encoder_options) => Some(AudioEncoder::new(
                output_id,
                audio_encoder_options,
                ctx,
                encoded_chunks_sender,
            )?),
            None => None,
        };

        Ok((
            Self {
                video: video_encoder,
                audio: audio_encoder,
            },
            encoded_chunks_receiver,
        ))
    }

    pub fn frame_sender(&self) -> Option<&Sender<PipelineEvent<Frame>>> {
        match &self.video {
            Some(VideoEncoder::H264(encoder)) => Some(encoder.frame_sender()),
            Some(VideoEncoder::VP8(encoder)) => Some(encoder.frame_sender()),
            Some(VideoEncoder::VP9(encoder)) => Some(encoder.frame_sender()),
            None => {
                error!("Non video encoder received frame to send.");
                None
            }
        }
    }

    pub fn keyframe_request_sender(&self) -> Option<Sender<()>> {
        match self.video.as_ref() {
            Some(VideoEncoder::H264(encoder)) => Some(encoder.keyframe_request_sender().clone()),
            Some(VideoEncoder::VP8(encoder)) => Some(encoder.keyframe_request_sender().clone()),
            Some(VideoEncoder::VP9(encoder)) => Some(encoder.keyframe_request_sender().clone()),
            None => {
                error!("Non video encoder received keyframe request.");
                None
            }
        }
    }

    pub fn samples_batch_sender(&self) -> Option<&Sender<PipelineEvent<OutputSamples>>> {
        match &self.audio {
            Some(encoder) => Some(encoder.samples_batch_sender()),
            None => {
                error!("Non audio encoder received samples to send.");
                None
            }
        }
    }

    pub fn context(&self) -> EncoderContext {
        EncoderContext {
            video: match &self.video {
                Some(VideoEncoder::H264(e)) => Some(VideoEncoderContext::H264(e.context())),
                Some(VideoEncoder::VP8(_)) => Some(VideoEncoderContext::VP8),
                Some(VideoEncoder::VP9(_)) => Some(VideoEncoderContext::VP9),
                None => None,
            },
            audio: match &self.audio {
                Some(AudioEncoder::Aac(e)) => Some(AudioEncoderContext::Aac(e.config.clone())),
                Some(AudioEncoder::Opus(_)) => Some(AudioEncoderContext::Opus),
                None => None,
            },
        }
    }
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

impl VideoEncoder {
    pub fn new(
        output_id: &OutputId,
        options: VideoEncoderOptions,
        ctx: &Arc<PipelineCtx>,
        sender: Sender<EncoderOutputEvent>,
    ) -> Result<Self, EncoderInitError> {
        match options {
            VideoEncoderOptions::H264(options) => Ok(Self::H264(LibavH264Encoder::new(
                output_id,
                options,
                ctx.output_framerate,
                sender,
            )?)),
            VideoEncoderOptions::VP8(options) => Ok(Self::VP8(LibavVP8Encoder::new(
                output_id,
                options,
                ctx.output_framerate,
                sender,
            )?)),
            VideoEncoderOptions::VP9(options) => Ok(Self::VP9(LibavVP9Encoder::new(
                output_id,
                options,
                ctx.output_framerate,
                sender,
            )?)),
        }
    }

    pub fn resolution(&self) -> Resolution {
        match self {
            Self::H264(encoder) => encoder.resolution(),
            Self::VP8(encoder) => encoder.resolution(),
            Self::VP9(encoder) => encoder.resolution(),
        }
    }

    pub fn pixel_format(&self) -> OutputFrameFormat {
        match self {
            Self::H264(encoder) => encoder.pixel_format(),
            Self::VP8(_) => OutputFrameFormat::PlanarYuv420Bytes,
            Self::VP9(encoder) => encoder.pixel_format(),
        }
    }

    pub fn keyframe_request_sender(&self) -> Sender<()> {
        match self {
            Self::H264(encoder) => encoder.keyframe_request_sender(),
            Self::VP8(encoder) => encoder.keyframe_request_sender(),
            Self::VP9(encoder) => encoder.keyframe_request_sender(),
        }
    }
}

impl AudioEncoder {
    fn new(
        output_id: &OutputId,
        options: AudioEncoderOptions,
        ctx: &Arc<PipelineCtx>,
        sender: Sender<EncoderOutputEvent>,
    ) -> Result<Self, EncoderInitError> {
        let resampler = if options.sample_rate() != ctx.mixing_sample_rate {
            Some(OutputResampler::new(
                ctx.mixing_sample_rate,
                options.sample_rate(),
            )?)
        } else {
            None
        };

        match options {
            AudioEncoderOptions::Opus(options) => {
                OpusEncoder::new(options, sender, resampler).map(AudioEncoder::Opus)
            }
            AudioEncoderOptions::Aac(options) => {
                AacEncoder::new(output_id, options, sender, resampler).map(AudioEncoder::Aac)
            }
        }
    }

    fn samples_batch_sender(&self) -> &Sender<PipelineEvent<OutputSamples>> {
        match self {
            Self::Opus(encoder) => encoder.samples_batch_sender(),
            Self::Aac(encoder) => encoder.samples_batch_sender(),
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
