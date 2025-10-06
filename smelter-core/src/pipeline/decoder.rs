use smelter_render::error::ErrorStack;
use std::iter;
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

use crossbeam_channel::Sender;
use smelter_render::Frame;

use crate::prelude::*;

pub(super) mod decoder_thread_audio;
pub(super) mod decoder_thread_video;
pub(super) mod dynamic_video_decoder;
pub(super) mod h264_utils;
pub(super) mod video_decoder_mapping;

mod ffmpeg_utils;

pub mod ffmpeg_h264;
pub mod ffmpeg_vp8;
pub mod ffmpeg_vp9;

#[cfg(feature = "vk-video")]
pub mod vulkan_h264;

#[cfg(not(feature = "vk-video"))]
#[path = "./decoder/vulkan_h264_fallback.rs"]
pub mod vulkan_h264;

pub mod fdk_aac;
pub mod libopus;

/// Raw samples produced by a decoder or received from external source.
/// They still need to be resampled before passing them to the queue.
#[derive(Debug)]
pub(super) struct DecodedSamples {
    pub samples: AudioSamples,
    pub start_pts: Duration,
    pub sample_rate: u32,
}

#[derive(Debug)]
pub(crate) struct DecoderThreadHandle {
    pub chunk_sender: Sender<PipelineEvent<EncodedInputChunk>>,
}

pub(crate) trait VideoDecoder: Sized + VideoDecoderInstance {
    const LABEL: &'static str;

    fn new(ctx: &Arc<PipelineCtx>) -> Result<Self, DecoderInitError>;
}

pub(crate) trait VideoDecoderInstance {
    fn decode(&mut self, chunk: EncodedInputChunk) -> Vec<Frame>;
    fn flush(&mut self) -> Vec<Frame>;
}

pub(crate) trait BytestreamTransformer: Send + 'static {
    fn transform(&mut self, data: bytes::Bytes) -> bytes::Bytes;
}

pub(crate) trait AudioDecoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError>;
    fn decode(&mut self, chunk: EncodedInputChunk) -> Result<Vec<DecodedSamples>, DecodingError>;
    fn flush(&mut self) -> Vec<DecodedSamples>;
}

pub(super) struct VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    decoder: Decoder,
    source: Source,
    eos_sent: bool,
}

impl<Decoder, Source> VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
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
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
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
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    decoder: Decoder,
    source: Source,
    eos_sent: bool,
}

impl<Decoder, Source> AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
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
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
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

pub struct BytestreamTransformStream<Source, Transformer>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
    Transformer: BytestreamTransformer,
{
    transformer: Option<Transformer>,
    source: Source,
    eos_sent: bool,
}

impl<Source, Transformer> BytestreamTransformStream<Source, Transformer>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
    Transformer: BytestreamTransformer,
{
    pub fn new(transformer: Option<Transformer>, source: Source) -> Self {
        Self {
            transformer,
            source,
            eos_sent: false,
        }
    }
}

impl<Source, Transformer> Iterator for BytestreamTransformStream<Source, Transformer>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
    Transformer: BytestreamTransformer,
{
    type Item = PipelineEvent<EncodedInputChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(mut chunk)) => {
                if let Some(ref mut transformer) = self.transformer {
                    chunk.data = transformer.transform(chunk.data);
                }
                Some(PipelineEvent::Data(chunk))
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    self.eos_sent = true;
                    Some(PipelineEvent::EOS)
                }
            },
        }
    }
}
