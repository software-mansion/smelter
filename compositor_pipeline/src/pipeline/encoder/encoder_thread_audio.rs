use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::prelude::*;
use crate::thread_utils::ThreadMetadata;
use crate::{
    pipeline::{
        encoder::{AudioEncoder, AudioEncoderConfig, AudioEncoderStream},
        resampler::encoder_resampler::ResampledForEncoderStream,
    },
    thread_utils::InitializableThread,
};

pub(crate) struct AudioEncoderThreadHandle {
    pub sample_batch_sender: Sender<PipelineEvent<OutputAudioSamples>>,
    pub config: AudioEncoderConfig,
}

pub(crate) struct AudioEncoderThreadOptions<Encoder: AudioEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub chunks_sender: Sender<EncodedOutputEvent>,
}

pub(crate) struct AudioEncoderThread<Encoder: AudioEncoder> {
    stream: Box<dyn Iterator<Item = EncodedOutputEvent>>,
    chunks_sender: Sender<EncodedOutputEvent>,
    _encoder: PhantomData<Encoder>,
}

impl<Encoder> InitializableThread for AudioEncoderThread<Encoder>
where
    Encoder: AudioEncoder + 'static,
{
    type InitOptions = AudioEncoderThreadOptions<Encoder>;

    type SpawnOutput = AudioEncoderThreadHandle;
    type SpawnError = EncoderInitError;

    const LABEL: &'static str = Encoder::LABEL;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let AudioEncoderThreadOptions {
            ctx,
            encoder_options,
            chunks_sender,
        } = options;

        let (sample_batch_sender, sample_batch_receiver) = crossbeam_channel::bounded(5);
        let resampled_stream = ResampledForEncoderStream::new(
            sample_batch_receiver.into_iter(),
            ctx.mixing_sample_rate,
            encoder_options.sample_rate(),
        )
        .flatten();

        let (encoded_stream, encoder_ctx) =
            AudioEncoderStream::<Encoder, _>::new(ctx, encoder_options, resampled_stream)?;

        let stream = encoded_stream.flatten().map(|event| match event {
            PipelineEvent::Data(chunk) => EncodedOutputEvent::Data(chunk),
            PipelineEvent::EOS => EncodedOutputEvent::AudioEOS,
        });

        let state = Self {
            stream: Box::new(stream),
            chunks_sender,
            _encoder: PhantomData,
        };
        let output = AudioEncoderThreadHandle {
            sample_batch_sender,
            config: encoder_ctx.config,
        };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.chunks_sender.send(event).is_err() {
                warn!("Failed to send encoded audio chunk from encoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: "Audio Encoder",
            thread_instance_name: "Output",
        }
    }
}

impl AudioEncoderThreadHandle {
    pub fn encoder_context(&self) -> Option<bytes::Bytes> {
        self.config.extradata.clone()
    }
}
