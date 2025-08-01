use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::prelude::*;
use crate::thread_utils::ThreadMetadata;
use crate::{
    pipeline::{
        decoder::{AudioDecoderStream, DecoderThreadHandle},
        resampler::decoder_resampler::ResampledDecoderStream,
    },
    thread_utils::InitializableThread,
};

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

    const LABEL: &'static str = Decoder::LABEL;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let AudioDecoderThreadOptions {
            ctx,
            decoder_options,
            samples_sender,
            input_buffer_size: buffer_size,
        } = options;

        let (chunk_sender, chunk_receiver) = crossbeam_channel::bounded(buffer_size);
        let output_sample_rate = ctx.mixing_sample_rate;

        let decoded_stream = AudioDecoderStream::<Decoder, _>::new(
            ctx,
            decoder_options,
            chunk_receiver.into_iter(),
        )?;

        let resampled_stream =
            ResampledDecoderStream::new(output_sample_rate, decoded_stream.flatten()).flatten();

        let state = Self {
            stream: Box::new(resampled_stream),
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
            thread_name: "Audio Decoder",
            thread_instance_name: "Input",
        }
    }
}
