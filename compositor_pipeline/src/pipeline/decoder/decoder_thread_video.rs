use std::{marker::PhantomData, sync::Arc};

use compositor_render::Frame;
use crossbeam_channel::Sender;
use tracing::warn;

use crate::{
    error::DecoderInitError,
    pipeline::decoder::{
        BytestreamTransformStream, BytestreamTransformer, DecoderThreadHandle, VideoDecoderStream,
    },
    thread_utils::InitializableThread,
    PipelineCtx, PipelineEvent,
};

use super::VideoDecoder;

pub(crate) struct VideoDecoderThreadOptions<Transformer: BytestreamTransformer> {
    pub ctx: Arc<PipelineCtx>,
    pub transformer: Option<Transformer>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub buffer_size: usize,
}

pub(crate) struct VideoDecoderThread<Decoder: VideoDecoder, Transformer: BytestreamTransformer> {
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

    type ThreadState = (
        Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
        Sender<PipelineEvent<Frame>>,
    );

    const LABEL: &'static str = Decoder::LABEL;

    fn init(
        options: Self::InitOptions,
    ) -> Result<(Self::SpawnOutput, Self::ThreadState), Self::SpawnError> {
        let VideoDecoderThreadOptions {
            ctx,
            transformer,
            frame_sender,
            buffer_size,
        } = options;
        let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(buffer_size);

        let transformed_bytestream =
            BytestreamTransformStream::new(transformer, chunk_receiver.into_iter());
        let decoder_stream = VideoDecoderStream::<Decoder, _>::new(ctx, transformed_bytestream)?;

        let output = DecoderThreadHandle { chunk_sender };
        let state = (Box::new(decoder_stream.flatten()) as Box<_>, frame_sender);
        Ok((output, state))
    }

    fn run(state: Self::ThreadState) {
        let (stream, frame_sender) = state;
        for event in stream {
            if frame_sender.send(event).is_err() {
                warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                return;
            }
        }
    }
}
