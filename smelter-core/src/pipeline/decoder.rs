use std::sync::Arc;

use crate::pipeline::utils::channel::Sender;

use crate::prelude::*;

pub(super) mod decoder_thread_audio;
pub(super) mod decoder_thread_video;

mod dynamic_stream;
mod static_stream;

pub(super) use dynamic_stream::{
    DynamicVideoDecoderStream, KeyframeRequestSender, VideoDecoderMapping,
};
pub(super) use static_stream::{AudioDecoderStream, VideoDecoderStream};

mod ffmpeg_utils;

pub mod ffmpeg_h264;
pub mod ffmpeg_vp8;
pub mod ffmpeg_vp9;

#[cfg(feature = "gpu-video")]
pub mod vulkan_h264;

#[cfg(not(feature = "gpu-video"))]
#[path = "./decoder/vulkan_h264_fallback.rs"]
pub mod vulkan_h264;

pub mod fdk_aac;
pub mod libopus;

#[derive(Debug)]
pub(crate) enum EncodedInputEvent {
    Chunk(EncodedInputChunk),
    LostData,
    AuDelimiter,
    /// What follows does not continue what came before: the input timeline was
    /// dropped, so state built from it (reference frames, partially parsed
    /// access units, codec parameters) does not describe what comes next.
    /// Everything the decoder still holds is decoded first.
    Discontinuity,
}

#[derive(Debug, Clone)]
pub(crate) struct DecoderThreadHandle {
    pub chunk_sender: Sender<PipelineEvent<EncodedInputEvent>>,
}

pub(crate) trait VideoDecoder: Sized + VideoDecoderInstance {
    const LABEL: &'static str;

    fn new(
        ctx: &Arc<PipelineCtx>,
        keyframe_request_sender: Option<KeyframeRequestSender>,
    ) -> Result<Self, DecoderInitError>;
}

pub(crate) trait VideoDecoderInstance {
    fn decode(&mut self, chunk: EncodedInputEvent) -> Vec<Frame>;
    fn flush(&mut self) -> Vec<Frame>;
}

pub(crate) trait BytestreamTransformer: Send + 'static {
    fn transform(&mut self, data: bytes::Bytes) -> bytes::Bytes;

    /// The stream continues on a new timeline, so anything the decoder needs
    /// to start over (e.g. parameter sets) has to be emitted again.
    fn on_discontinuity(&mut self) {}
}

pub(crate) trait AudioDecoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError>;
    fn decode(&mut self, event: EncodedInputEvent)
    -> Result<Vec<InputAudioSamples>, DecodingError>;
    fn flush(&mut self) -> Vec<InputAudioSamples>;
}

pub(crate) struct BytestreamTransformStream<Source, Transformer>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
    Transformer: BytestreamTransformer,
{
    transformer: Option<Transformer>,
    source: Source,
    eos_sent: bool,
}

impl<Source, Transformer> BytestreamTransformStream<Source, Transformer>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
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
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
    Transformer: BytestreamTransformer,
{
    type Item = PipelineEvent<EncodedInputEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(EncodedInputEvent::Chunk(mut chunk))) => {
                if let Some(ref mut transformer) = self.transformer {
                    chunk.data = transformer.transform(chunk.data);
                }
                Some(PipelineEvent::Data(EncodedInputEvent::Chunk(chunk)))
            }
            Some(PipelineEvent::Data(EncodedInputEvent::Discontinuity)) => {
                if let Some(ref mut transformer) = self.transformer {
                    transformer.on_discontinuity();
                }
                Some(PipelineEvent::Data(EncodedInputEvent::Discontinuity))
            }
            Some(PipelineEvent::Data(event)) => Some(PipelineEvent::Data(event)),
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
