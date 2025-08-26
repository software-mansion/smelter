use std::marker::PhantomData;
use std::sync::Arc;

use crossbeam_channel::Sender;
use tracing::warn;

use crate::prelude::*;
use crate::{
    error::EncoderInitError,
    pipeline::{
        encoder::{AudioEncoder, AudioEncoderStream},
        resampler::encoder_resampler::ResampledForEncoderStream,
        rtp::payloader::{PayloaderOptions, PayloaderStream},
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

use super::RtpEvent;

pub(crate) struct RtpAudioTrackThreadHandle {
    pub sample_batch_sender: Sender<PipelineEvent<OutputAudioSamples>>,
}

pub(super) struct RtpAudioTrackThreadOptions<Encoder: AudioEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub payloader_options: PayloaderOptions,
    pub chunks_sender: Sender<RtpEvent>,
}

pub(super) struct RtpAudioTrackThread<Encoder: AudioEncoder> {
    stream: Box<dyn Iterator<Item = RtpEvent>>,
    chunks_sender: Sender<RtpEvent>,
    _encoder: PhantomData<Encoder>,
}

impl<Encoder> InitializableThread for RtpAudioTrackThread<Encoder>
where
    Encoder: AudioEncoder + 'static,
{
    type InitOptions = RtpAudioTrackThreadOptions<Encoder>;

    type SpawnOutput = RtpAudioTrackThreadHandle;
    type SpawnError = EncoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let RtpAudioTrackThreadOptions {
            ctx,
            encoder_options,
            payloader_options,
            chunks_sender,
        } = options;

        let ssrc = payloader_options.ssrc;
        let (sample_batch_sender, sample_batch_receiver) = crossbeam_channel::bounded(5);

        let resampled_stream = ResampledForEncoderStream::new(
            sample_batch_receiver.into_iter(),
            ctx.mixing_sample_rate,
            encoder_options.sample_rate(),
        )
        .flatten();

        let (encoded_stream, _encoder_ctx) =
            AudioEncoderStream::<Encoder, _>::new(ctx, encoder_options, resampled_stream)?;

        let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

        let stream = payloaded_stream.flatten().map(move |event| match event {
            Ok(PipelineEvent::Data(packet)) => RtpEvent::Data(packet),
            Ok(PipelineEvent::EOS) => RtpEvent::AudioEos(webrtc::rtcp::goodbye::Goodbye {
                sources: vec![ssrc],
                reason: bytes::Bytes::from("Unregister output stream"),
            }),
            Err(err) => RtpEvent::Err(err),
        });

        let state = Self {
            stream: Box::new(stream),
            chunks_sender,
            _encoder: PhantomData,
        };
        let output = RtpAudioTrackThreadHandle {
            sample_batch_sender,
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
            thread_name: format!("Rtp Audio Encoder ({})", Encoder::LABEL),
            thread_instance_name: "Output".to_string(),
        }
    }
}
