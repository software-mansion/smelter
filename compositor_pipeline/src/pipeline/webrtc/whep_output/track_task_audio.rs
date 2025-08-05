use std::{marker::PhantomData, sync::Arc};

use compositor_render::error::ErrorStack;
use tokio::sync::{broadcast, watch};
use tracing::warn;

use crate::prelude::*;
use crate::{
    pipeline::{
        encoder::{AudioEncoder, AudioEncoderStream},
        resampler::encoder_resampler::ResampledForEncoderStream,
        rtp::{
            payloader::{PayloaderOptions, PayloaderStream},
            RtpPacket,
        },
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

#[derive(Debug, Clone)]
pub(crate) struct WhepAudioTrackThreadHandle {
    pub sample_batch_sender: crossbeam_channel::Sender<PipelineEvent<OutputAudioSamples>>,
    pub packet_loss_sender: watch::Sender<i32>,
}

pub(super) struct WhepAudioTrackThreadOptions<Encoder: AudioEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub payloader_options: PayloaderOptions,
    pub chunks_sender: broadcast::Sender<RtpPacket>,
}

pub(super) struct WhepAudioTrackThread<Encoder: AudioEncoder> {
    stream: Box<dyn Iterator<Item = RtpPacket>>,
    chunks_sender: broadcast::Sender<RtpPacket>,
    _encoder: PhantomData<Encoder>,
}

impl<Encoder> InitializableThread for WhepAudioTrackThread<Encoder>
where
    Encoder: AudioEncoder + 'static,
{
    type InitOptions = WhepAudioTrackThreadOptions<Encoder>;

    type SpawnOutput = WhepAudioTrackThreadHandle;
    type SpawnError = EncoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let WhepAudioTrackThreadOptions {
            ctx,
            encoder_options,
            payloader_options,
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

        let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

        let stream = payloaded_stream.flatten().filter_map(|event| match event {
            Ok(PipelineEvent::Data(packet)) => Some(packet),
            Ok(PipelineEvent::EOS) => None,
            Err(err) => {
                warn!(
                    "Depayloading error: {}",
                    ErrorStack::new(&err).into_string()
                );
                None
            }
        });

        let state = Self {
            stream: Box::new(stream),
            chunks_sender,
            _encoder: PhantomData,
        };
        let output = WhepAudioTrackThreadHandle {
            sample_batch_sender,
            packet_loss_sender: encoder_ctx.packet_loss_sender,
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
            thread_name: "Whep Audio Encoder".to_string(),
            thread_instance_name: "Output".to_string(),
        }
    }
}
