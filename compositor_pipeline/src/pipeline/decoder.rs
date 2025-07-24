use compositor_render::error::ErrorStack;
use std::iter;
use std::sync::Arc;
use tracing::warn;

use crate::error::DecoderInitError;
use crate::pipeline::types::DecodedSamples;
use crate::pipeline::{EncodedChunk, PipelineCtx};
use crate::{audio_mixer::InputSamples, queue::PipelineEvent};

use compositor_render::Frame;
use crossbeam_channel::{Receiver, Sender};

pub(super) mod decoder_thread_audio;
pub(super) mod decoder_thread_video;

pub mod ffmpeg_h264;
pub mod ffmpeg_vp8;
pub mod ffmpeg_vp9;

#[cfg(feature = "vk-video")]
pub mod vulkan_h264;

#[cfg(not(feature = "vk-video"))]
#[path = "./decoder/vulkan_h264_fallback.rs"]
pub mod vulkan_h264;

pub mod h264_utils;

pub mod fdk_aac;
pub mod libopus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoDecoderOptions {
    FfmpegH264,
    FfmpegVp8,
    FfmpegVp9,
    VulkanH264,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDecoderOptions {
    Opus,
    FdkAac(fdk_aac::Options),
}

#[derive(Debug)]
pub struct DecodedDataReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<InputSamples>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecodingError {
    #[error(transparent)]
    OpusError(#[from] libopus::LibOpusError),
    #[error(transparent)]
    AacDecoder(#[from] fdk_aac::FdkAacDecoderError),
}

#[derive(Debug)]
pub(crate) struct DecoderThreadHandle {
    pub chunk_sender: Sender<PipelineEvent<EncodedChunk>>,
}

pub(crate) trait VideoDecoder: Sized + VideoDecoderInstance {
    const LABEL: &'static str;

    fn new(ctx: &Arc<PipelineCtx>) -> Result<Self, DecoderInitError>;
}

pub(crate) trait VideoDecoderInstance {
    fn decode(&mut self, chunk: EncodedChunk) -> Vec<Frame>;
    fn flush(&mut self) -> Vec<Frame>;
}

pub(crate) trait AudioDecoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError>;
    fn decode(&mut self, chunk: EncodedChunk) -> Result<Vec<DecodedSamples>, DecodingError>;
    fn flush(&mut self) -> Vec<DecodedSamples>;
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
                let result = self.decoder.decode(samples);
                let chunks = match result {
                    Ok(chunks) => chunks,
                    Err(err) => {
                        warn!(
                            "Audio decoder error: {}",
                            ErrorStack::new(&err).into_string()
                        );
                        return Some(vec![]);
                    }
                };
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
