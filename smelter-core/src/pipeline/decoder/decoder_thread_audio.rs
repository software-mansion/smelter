use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::{
    pipeline::decoder::{AudioDecoderStream, DecoderThreadHandle, EncodedInputEvent},
    thread_utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

use super::AudioDecoder;

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

        let state = Self {
            stream: Box::new(decoded_stream.flatten()),
            samples_sender,
            _decoder: PhantomData,
        };
        let output = DecoderThreadHandle { chunk_sender };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.samples_sender.send(event).is_err() {
                warn!("Failed to send encoded audio chunk from decoder. Channel closed.");
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
