use std::{iter, sync::Arc};

use compositor_render::{Frame, OutputFrameFormat, Resolution};
use crossbeam_channel::{unbounded, Receiver, Sender};
use ffmpeg_next::format::Pixel;
use resampler::OutputResampler;

use crate::{
    audio_mixer::{AudioChannels, OutputSamples},
    error::EncoderInitError,
    queue::PipelineEvent,
};

use super::{EncodedChunk, PipelineCtx};

pub(crate) mod encoder_thread_audio;
pub(crate) mod encoder_thread_video;

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

#[derive(Debug)]
pub(crate) struct VideoEncoderConfig {
    pub resolution: Resolution,
    pub output_format: OutputFrameFormat,
    pub extradata: Option<bytes::Bytes>,
}

pub(crate) trait VideoEncoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError>;
    fn encode(&mut self, frame: Frame, force_keyframe: bool) -> Vec<EncodedChunk>;
    fn flush(&mut self) -> Vec<EncodedChunk>;
}

#[derive(Debug)]
pub(crate) struct AudioEncoderConfig {
    //pub channels: AudioChannels,
    //pub sample_rate: u32,
    pub extradata: Option<bytes::Bytes>,
}

pub(crate) trait AudioEncoderOptionsExt {
    fn sample_rate(&self) -> u32;
}

pub(crate) trait AudioEncoder: Sized {
    const LABEL: &'static str;

    type Options: AudioEncoderOptionsExt + Send + 'static;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, AudioEncoderConfig), EncoderInitError>;
    fn encode(&mut self, samples: OutputSamples) -> Vec<EncodedChunk>;
    fn flush(&mut self) -> Vec<EncodedChunk>;
}

pub(super) struct VideoEncoderStreamContext {
    pub keyframe_request_sender: Sender<()>,
    pub config: VideoEncoderConfig,
}

pub(super) struct VideoEncoderStream<Encoder, Source>
where
    Encoder: VideoEncoder,
    Source: Iterator<Item = PipelineEvent<Frame>>,
{
    encoder: Encoder,
    source: Source,
    keyframe_request_receiver: Receiver<()>,
    eos_sent: bool,
}

impl<Encoder, Source> VideoEncoderStream<Encoder, Source>
where
    Encoder: VideoEncoder,
    Source: Iterator<Item = PipelineEvent<Frame>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Encoder::Options,
        source: Source,
    ) -> Result<(Self, VideoEncoderStreamContext), EncoderInitError> {
        let (keyframe_request_sender, keyframe_request_receiver) = unbounded();
        let (encoder, config) = Encoder::new(&ctx, options)?;
        Ok((
            Self {
                encoder,
                source,
                eos_sent: false,
                keyframe_request_receiver,
            },
            VideoEncoderStreamContext {
                keyframe_request_sender,
                config,
            },
        ))
    }

    fn has_keyframe_request(&self) -> bool {
        let mut has_keyframe_request = false;
        while self.keyframe_request_receiver.try_recv().is_ok() {
            has_keyframe_request = true;
        }
        has_keyframe_request
    }
}

impl<Encoder, Source> Iterator for VideoEncoderStream<Encoder, Source>
where
    Encoder: VideoEncoder,
    Source: Iterator<Item = PipelineEvent<Frame>>,
{
    type Item = Vec<PipelineEvent<EncodedChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(frame)) => {
                let chunks = self.encoder.encode(frame, self.has_keyframe_request());
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self.encoder.flush();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}

pub(super) struct AudioEncoderStream<Encoder, Source>
where
    Encoder: AudioEncoder,
    Source: Iterator<Item = PipelineEvent<OutputSamples>>,
{
    encoder: Encoder,
    source: Source,
    eos_sent: bool,
}

impl<Encoder, Source> AudioEncoderStream<Encoder, Source>
where
    Encoder: AudioEncoder,
    Source: Iterator<Item = PipelineEvent<OutputSamples>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Encoder::Options,
        source: Source,
    ) -> Result<(Self, AudioEncoderConfig), EncoderInitError> {
        let (encoder, config) = Encoder::new(&ctx, options)?;
        Ok((
            Self {
                encoder,
                source,
                eos_sent: false,
            },
            config,
        ))
    }
}

impl<Encoder, Source> Iterator for AudioEncoderStream<Encoder, Source>
where
    Encoder: AudioEncoder,
    Source: Iterator<Item = PipelineEvent<OutputSamples>>,
{
    type Item = Vec<PipelineEvent<EncodedChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let chunks = self.encoder.encode(samples);
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self.encoder.flush();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}

pub(super) struct ResampledStream<Source: Iterator<Item = PipelineEvent<OutputSamples>>> {
    resampler: Option<OutputResampler>,
    source: Source,
    eos_sent: bool,
}

impl<Source: Iterator<Item = PipelineEvent<OutputSamples>>> ResampledStream<Source> {
    pub fn new(
        source: Source,
        input_sample_rate: u32,
        output_sample_rate: u32,
    ) -> Result<Self, EncoderInitError> {
        let resampler = match input_sample_rate != output_sample_rate {
            true => Some(OutputResampler::new(input_sample_rate, output_sample_rate)?),
            false => None,
        };
        Ok(Self {
            resampler,
            source,
            eos_sent: false,
        })
    }
}

impl<Source: Iterator<Item = PipelineEvent<OutputSamples>>> Iterator for ResampledStream<Source> {
    type Item = Vec<PipelineEvent<OutputSamples>>;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(resampler) = &mut self.resampler else {
            return self.source.next().map(|event| vec![event]);
        };
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let resampled = resampler.resample(samples);
                Some(resampled.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    self.eos_sent = true;
                    Some(vec![PipelineEvent::EOS])
                }
            },
        }
    }
}
