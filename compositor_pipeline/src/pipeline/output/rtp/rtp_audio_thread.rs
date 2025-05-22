use std::sync::Arc;

use compositor_render::OutputId;
use crossbeam_channel::Sender;
use tracing::{debug, span, warn, Level};

use crate::{
    audio_mixer::{AudioChannels, OutputSamples},
    error::EncoderInitError,
    pipeline::{
        encoder::{
            AudioEncoder, AudioEncoderConfig, AudioEncoderOptionsExt, AudioEncoderStream,
            ResampledStream,
        },
        PipelineCtx,
    },
    queue::PipelineEvent,
};

use super::{
    payloader::{PayloaderOptions, PayloaderStream},
    RtpEvent,
};

pub(crate) struct RtpAudioTrackThreadHandle {
    sample_batch_sender: Sender<PipelineEvent<OutputSamples>>,
    config: AudioEncoderConfig,
}

pub fn spawn_rtp_audio_thread<Encoder: AudioEncoder>(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
    chunks_sender: Sender<RtpEvent>,
) -> Result<RtpAudioTrackThreadHandle, EncoderInitError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("RTP audio track thread for output {}", &output_id))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "Encoder thread",
                output_id = output_id.to_string(),
                encoder = Encoder::LABEL
            )
            .entered();

            let result = init_stream::<Encoder>(ctx, encoder_options, payloader_options);
            let stream = match result {
                Ok((stream, handle)) => {
                    result_sender.send(Ok(handle)).unwrap();
                    stream
                }
                Err(err) => {
                    result_sender.send(Err(err)).unwrap();
                    return;
                }
            };
            for event in stream {
                if chunks_sender.send(event).is_err() {
                    warn!("Failed to send encoded audio chunk from encoder. Channel closed.");
                    return;
                }
            }
            debug!("Encoder thread finished.");
        })
        .unwrap();

    result_receiver.recv().unwrap()
}

fn init_stream<Encoder: AudioEncoder>(
    ctx: Arc<PipelineCtx>,
    encoder_options: Encoder::Options,
    payloader_options: PayloaderOptions,
) -> Result<(impl Iterator<Item = RtpEvent>, RtpAudioTrackThreadHandle), EncoderInitError> {
    let (sample_batch_sender, sample_batch_receiver) = crossbeam_channel::bounded(5);

    let resampled_stream = ResampledStream::new(
        sample_batch_receiver.into_iter(),
        ctx.mixing_sample_rate,
        encoder_options.sample_rate(),
    )?
    .flatten();

    let (encoded_stream, config) =
        AudioEncoderStream::<Encoder, _>::new(ctx, encoder_options, resampled_stream)?;

    let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

    let stream = payloaded_stream.flatten().map(|event| match event {
        Ok(PipelineEvent::Data(packet)) => RtpEvent::Data(packet),
        Ok(PipelineEvent::EOS) => RtpEvent::AudioEos,
        Err(err) => RtpEvent::Err(err),
    });

    Ok((
        stream,
        RtpAudioTrackThreadHandle {
            sample_batch_sender,
            config,
        },
    ))
}

impl RtpAudioTrackThreadHandle {
    pub fn sample_batch_sender(&self) -> &Sender<PipelineEvent<OutputSamples>> {
        &self.sample_batch_sender
    }

    pub fn channels(&self) -> AudioChannels {
        self.config.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    pub fn encoder_context(&self) -> Option<bytes::Bytes> {
        self.config.extradata.clone()
    }
}
