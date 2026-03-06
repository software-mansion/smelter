use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::{
    pipeline::encoder::{
        AudioEncoder, AudioEncoderConfig, AudioEncoderStream, resampler::ResampledForEncoderStream,
    },
    utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub(crate) struct AudioEncoderThreadHandle {
    pub sample_batch_sender: Sender<PipelineEvent<OutputAudioSamples>>,
    pub config: AudioEncoderConfig,
}

pub(crate) struct AudioEncoderThreadOptions<Encoder: AudioEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub chunks_sender: Sender<EncodedOutputEvent>,
    pub output_ref: Ref<OutputId>,
    pub chunk_size_event: Option<fn(u64, &Ref<OutputId>) -> StatsEvent>,
}

pub(crate) struct AudioEncoderThread<Encoder: AudioEncoder> {
    stream: Box<dyn Iterator<Item = EncodedOutputEvent>>,
    chunks_sender: Sender<EncodedOutputEvent>,
    output_ref: Ref<OutputId>,
    stats_sender: StatsSender,
    chunk_size_event: Option<fn(u64, &Ref<OutputId>) -> StatsEvent>,
    _encoder: PhantomData<Encoder>,
}

impl<Encoder> InitializableThread for AudioEncoderThread<Encoder>
where
    Encoder: AudioEncoder + 'static,
{
    type InitOptions = AudioEncoderThreadOptions<Encoder>;

    type SpawnOutput = AudioEncoderThreadHandle;
    type SpawnError = EncoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let AudioEncoderThreadOptions {
            ctx,
            encoder_options,
            chunks_sender,
            output_ref,
            chunk_size_event,
        } = options;
        let stats_sender = ctx.stats_sender.clone();

        let (sample_batch_sender, sample_batch_receiver) = crossbeam_channel::bounded(5);
        let resampled_stream = ResampledForEncoderStream::new(
            sample_batch_receiver.into_iter(),
            ctx.mixing_sample_rate,
            encoder_options.sample_rate(),
            encoder_options.channels(),
        )?
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
            output_ref,
            stats_sender,
            chunk_size_event,
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
            if let Some(make_event) = self.chunk_size_event {
                self.stats_sender
                    .send(vec![make_event(event.data_size(), &self.output_ref)]);
            }
            if self.chunks_sender.send(event).is_err() {
                warn!("Failed to send encoded audio chunk from encoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: format!("Audio Encoder ({})", Encoder::LABEL),
            thread_instance_name: "Output".to_string(),
        }
    }
}

impl AudioEncoderThreadHandle {
    pub fn encoder_context(&self) -> Option<bytes::Bytes> {
        self.config.extradata.clone()
    }
}
