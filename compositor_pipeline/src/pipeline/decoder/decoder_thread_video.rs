use std::{marker::PhantomData, sync::Arc};

use compositor_render::Frame;
use crossbeam_channel::Sender;
use tracing::warn;

use crate::{
    error::DecoderInitError,
    pipeline::decoder::{
        BytestreamTransformStream, BytestreamTransformer, DecoderThreadHandle, VideoDecoderStream,
    },
    thread_utils::{InitializableThread, ThreadMetadata},
    PipelineCtx, PipelineEvent,
};

use super::VideoDecoder;

pub(crate) struct VideoDecoderThreadOptions<Transformer: BytestreamTransformer> {
    pub ctx: Arc<PipelineCtx>,
    pub transformer: Option<Transformer>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_buffer_size: usize,
}

pub(crate) struct VideoDecoderThread<Decoder: VideoDecoder, Transformer: BytestreamTransformer> {
    stream: Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    _decoder: PhantomData<Decoder>,
    _transformer: PhantomData<Transformer>,
}

impl<Decoder, Transformer> InitializableThread for VideoDecoderThread<Decoder, Transformer>
where
    Decoder: VideoDecoder + 'static,
    Transformer: BytestreamTransformer,
{
    type InitOptions = VideoDecoderThreadOptions<Transformer>;

    type SpawnOutput = DecoderThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let VideoDecoderThreadOptions {
            ctx,
            transformer,
            frame_sender,
            input_buffer_size: buffer_size,
        } = options;
        let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(buffer_size);

        let transformed_bytestream =
            BytestreamTransformStream::new(transformer, chunk_receiver.into_iter());
        let decoder_stream = VideoDecoderStream::<Decoder, _>::new(ctx, transformed_bytestream)?;

        let state = Self {
            stream: Box::new(decoder_stream.flatten()),
            frame_sender,
            _decoder: PhantomData,
            _transformer: PhantomData,
        };
        let output = DecoderThreadHandle { chunk_sender };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.frame_sender.send(event).is_err() {
                warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: format!("Video Decoder ({})", Decoder::LABEL),
            thread_instance_name: "Input".to_string(),
        }
    }
}
