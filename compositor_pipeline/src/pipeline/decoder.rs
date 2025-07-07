use std::iter;
use std::sync::Arc;

use crate::error::DecoderInitError;
use crate::pipeline::decoder::audio::DecodingError;
use crate::pipeline::types::DecodedSamples;
use crate::pipeline::{EncodedChunk, PipelineCtx};
use crate::{audio_mixer::InputSamples, queue::PipelineEvent};

use compositor_render::Frame;
use crossbeam_channel::Receiver;

pub use audio::AacDecoderError;

mod audio;
mod video;

mod decoder_thread_audio;
mod decoder_thread_video;

pub mod ffmpeg_h264;
pub mod ffmpeg_vp8;
pub mod ffmpeg_vp9;
pub mod vulkan_h264;

pub mod fdk_aac;
pub mod opus;

pub(super) use audio::start_audio_decoder_thread;
pub(super) use audio::start_audio_resampler_only_thread;
pub(super) use video::start_video_decoder_thread;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoDecoderOptions {
    FfmpegH264,
    FfmpegVp8,
    FfmpegVp9,
    VulkanH264,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDecoderOptions {
    Opus(opus::Options),
    FdkAac(fdk_aac::Options),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpusDecoderOptions {
    pub forward_error_correction: bool,
}

#[derive(Debug)]
pub struct DecodedDataReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<InputSamples>>>,
}

pub(crate) trait VideoDecoder: Sized {
    const LABEL: &'static str;

    fn new(ctx: &Arc<PipelineCtx>) -> Result<Self, DecoderInitError>;
    fn decode(&mut self, chunk: EncodedChunk) -> Vec<Frame>;
    fn flush(&mut self) -> Vec<Frame>;
}

pub(crate) trait AudioDecoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError>;
    fn decode(&mut self, chunk: EncodedChunk) -> Result<Vec<DecodedSamples>, DecodingError>;
    fn flush(&mut self) -> Vec<InputSamples>;
}

pub(super) struct VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    decoder: Decoder,
    source: Source,
    eos_sent: bool,
}

impl<Decoder, Source> VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    pub fn new(ctx: Arc<PipelineCtx>, source: Source) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx)?;
        Ok(Self {
            decoder,
            source,
            eos_sent: false,
        })
    }
}

impl<Decoder, Source> Iterator for VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    type Item = Vec<PipelineEvent<Frame>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let chunks = self.decoder.decode(samples);
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self.decoder.flush();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}

pub(super) struct AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    decoder: Decoder,
    source: Source,
    eos_sent: bool,
}

impl<Decoder, Source> AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Decoder::Options,
        source: Source,
    ) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx, options)?;
        Ok(Self {
            decoder,
            source,
            eos_sent: false,
        })
    }
}

impl<Decoder, Source> Iterator for AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    type Item = Vec<PipelineEvent<DecodedSamples>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let chunks = self.decoder.decode(samples);
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self.decoder.flush();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}
