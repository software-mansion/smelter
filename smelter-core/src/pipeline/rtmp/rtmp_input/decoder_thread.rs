use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::{
    PipelineCtx, PipelineEvent,
    error::DecoderInitError,
    pipeline::decoder::{
        AudioDecoder, AudioDecoderStream, BytestreamTransformStream, BytestreamTransformer,
        DecoderThreadHandle, EncodedInputEvent, VideoDecoder, VideoDecoderStream,
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

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
            BytestreamTransformStream::new(transformer, chunk_receiver.into_iter()).map(|event| {
                match event {
                    PipelineEvent::Data(chunk) => {
                        PipelineEvent::Data(EncodedInputEvent::Chunk(chunk))
                    }
                    PipelineEvent::EOS => PipelineEvent::EOS,
                }
            });

        let decoder_stream = VideoDecoderStream::<Decoder, _>::new(ctx, transformed_bytestream)?;

        let result_stream = decoder_stream.flatten().filter_map(|event| match event {
            PipelineEvent::Data(frame) => Some(PipelineEvent::Data(frame)),
            // Do not send EOS to queue to allow reconnects on the same input.
            // TODO: maybe queue should be able to handle packets after EOS
            PipelineEvent::EOS => None,
        });

        let state = Self {
            stream: Box::new(result_stream),
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
                warn!("Failed to send decoded video chunk from decoder. Channel closed.");
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

pub(crate) struct AudioDecoderThreadOptions<Decoder: AudioDecoder> {
    pub ctx: Arc<PipelineCtx>,
    pub decoder_options: Decoder::Options,
    pub samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub input_buffer_size: usize,
}

pub(crate) struct AudioDecoderThread<Decoder: AudioDecoder> {
    stream: Box<dyn Iterator<Item = PipelineEvent<InputAudioSamples>>>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    _decoder: PhantomData<Decoder>,
}

impl<Decoder> InitializableThread for AudioDecoderThread<Decoder>
where
    Decoder: AudioDecoder + 'static,
{
    type InitOptions = AudioDecoderThreadOptions<Decoder>;

    type SpawnOutput = DecoderThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let AudioDecoderThreadOptions {
            ctx,
            decoder_options,
            samples_sender,
            input_buffer_size: buffer_size,
        } = options;

        let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(buffer_size);

        let chunk_stream = chunk_receiver.into_iter().map(|event| match event {
            PipelineEvent::Data(chunk) => PipelineEvent::Data(EncodedInputEvent::Chunk(chunk)),
            PipelineEvent::EOS => PipelineEvent::EOS,
        });

        let decoded_stream =
            AudioDecoderStream::<Decoder, _>::new(ctx, decoder_options, chunk_stream)?;

        let result_stream = decoded_stream.flatten().filter_map(|event| match event {
            PipelineEvent::Data(batch) => Some(PipelineEvent::Data(batch)),
            // Do not send EOS to queue to allow reconnects on the same input.
            // TODO: maybe queue should be able to handle packets after EOS
            PipelineEvent::EOS => None,
        });

        let state = Self {
            stream: Box::new(result_stream),
            samples_sender,
            _decoder: PhantomData,
        };
        let output = DecoderThreadHandle { chunk_sender };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.samples_sender.send(event).is_err() {
                warn!("Failed to send decoded audio chunk from decoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: format!("Audio Decoder ({})", Decoder::LABEL),
            thread_instance_name: "Input".to_string(),
        }
    }
}
