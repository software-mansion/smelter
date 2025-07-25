use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::prelude::*;
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
    pub buffer_size: usize,
}

pub(crate) struct AudioDecoderThread<Decoder: AudioDecoder> {
    _decoder: PhantomData<Decoder>,
}

impl<Decoder> InitializableThread for AudioDecoderThread<Decoder>
where
    Decoder: AudioDecoder + 'static,
{
    type InitOptions = AudioDecoderThreadOptions<Decoder>;

    type SpawnOutput = DecoderThreadHandle;
    type SpawnError = DecoderInitError;

    type ThreadState = (
        Box<dyn Iterator<Item = PipelineEvent<InputAudioSamples>>>,
        Sender<PipelineEvent<InputAudioSamples>>,
    );

    const LABEL: &'static str = Decoder::LABEL;

    fn init(
        options: Self::InitOptions,
    ) -> Result<(Self::SpawnOutput, Self::ThreadState), Self::SpawnError> {
        let AudioDecoderThreadOptions {
            ctx,
            decoder_options,
            samples_sender,
            buffer_size,
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

        let output = DecoderThreadHandle { chunk_sender };
        let state = (Box::new(resampled_stream) as Box<_>, samples_sender);
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
