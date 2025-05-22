use std::sync::Arc;

use compositor_render::OutputId;
use crossbeam_channel::{Receiver, Sender};
use tracing::{debug, span, warn, Level};

use crate::{
    audio_mixer::{AudioChannels, OutputSamples},
    error::EncoderInitError,
    pipeline::{EncoderOutputEvent, PipelineCtx},
    queue::PipelineEvent,
};

use super::{resampler::OutputResampler, AudioEncoder, AudioEncoderConfig};

pub(crate) struct AudioEncoderThread<Encoder: AudioEncoder> {
    ctx: Arc<PipelineCtx>,
    encoder: Encoder,
    sample_batch_receiver: Receiver<PipelineEvent<OutputSamples>>,
    chunks_sender: Sender<EncoderOutputEvent>,
    resampler: Option<OutputResampler>,
}

pub(crate) struct AudioEncoderThreadHandle {
    sample_batch_sender: Sender<PipelineEvent<OutputSamples>>,
    config: AudioEncoderConfig,
}

impl<Encoder: AudioEncoder> AudioEncoderThread<Encoder> {
    pub fn spawn(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: Encoder::Options,
        chunks_sender: Sender<EncoderOutputEvent>,
    ) -> Result<AudioEncoderThreadHandle, EncoderInitError> {
        let (sample_batch_sender, sample_batch_receiver) = crossbeam_channel::bounded(5);
        let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

        std::thread::Builder::new()
            .name(format!("Encoder thread for output {}", &output_id))
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "Encoder thread",
                    output_id = output_id.to_string(),
                    encoder = Encoder::LABEL
                )
                .entered();

                let result = Self::new(ctx, options, chunks_sender, sample_batch_receiver);
                match result {
                    Ok((encoder, config)) => {
                        result_sender.send(Ok(config)).unwrap();
                        encoder.run()
                    }
                    Err(err) => {
                        result_sender.send(Err(err)).unwrap();
                    }
                };
            })
            .unwrap();

        let config = result_receiver.recv().unwrap()?;

        Ok(AudioEncoderThreadHandle {
            sample_batch_sender,
            config,
        })
    }

    fn new(
        ctx: Arc<PipelineCtx>,
        options: Encoder::Options,
        chunks_sender: Sender<EncoderOutputEvent>,
        sample_batch_receiver: Receiver<PipelineEvent<OutputSamples>>,
    ) -> Result<(Self, AudioEncoderConfig), EncoderInitError> {
        let (encoder, config) = Encoder::new(&ctx, options)?;
        let resampler = match config.sample_rate != ctx.mixing_sample_rate {
            true => Some(OutputResampler::new(
                ctx.mixing_sample_rate,
                config.sample_rate,
            )?),
            false => None,
        };
        Ok((
            Self {
                ctx,
                encoder,
                chunks_sender,
                sample_batch_receiver,
                resampler,
            },
            config,
        ))
    }

    fn run(mut self) {
        loop {
            let sample_batch = match self.sample_batch_receiver.recv() {
                Ok(PipelineEvent::Data(f)) => f,
                Ok(PipelineEvent::EOS) => break,
                Err(_) => break,
            };
            let resampled_batches = match &mut self.resampler {
                Some(resampler) => resampler.resample(sample_batch),
                None => vec![sample_batch],
            };
            for sample_batch in resampled_batches {
                let chunks = self.encoder.encode(sample_batch);
                for chunk in chunks {
                    if self
                        .chunks_sender
                        .send(EncoderOutputEvent::Data(chunk))
                        .is_err()
                    {
                        warn!("Failed to send encoded audio chunk from encoder. Channel closed.");
                        return;
                    }
                }
            }
        }

        let flushed = self.encoder.flush();
        for chunk in flushed {
            if self
                .chunks_sender
                .send(EncoderOutputEvent::Data(chunk))
                .is_err()
            {
                warn!("Failed to send encoded video. Channel closed.");
                return;
            }
        }
        if let Err(_err) = self.chunks_sender.send(EncoderOutputEvent::AudioEOS) {
            warn!("Failed to send EOS. Channel closed.")
        }
        debug!("Encoder thread finished.");
    }
}

impl AudioEncoderThreadHandle {
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
