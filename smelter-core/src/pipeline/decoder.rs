use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;
use smelter_render::Frame;

use crate::prelude::*;

pub(super) mod decoder_thread_audio;
pub(super) mod decoder_thread_video;
pub(super) mod h264_utils;

mod dynamic_stream;
mod static_stream;

pub(super) use dynamic_stream::{DynamicVideoDecoderStream, VideoDecoderMapping};
pub(super) use static_stream::{AudioDecoderStream, VideoDecoderStream};

mod dynamic_stream;
mod static_stream;

pub(super) use dynamic_stream::{DynamicVideoDecoderStream, VideoDecoderMapping};
pub(super) use static_stream::{AudioDecoderStream, VideoDecoderStream};

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
pub(crate) struct DecodedSamples {
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
