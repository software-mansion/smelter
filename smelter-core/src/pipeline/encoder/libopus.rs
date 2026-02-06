use std::{sync::Arc, time::Duration};

use audioadapter::Adapter;
use bytes::Bytes;
use tracing::{error, info, trace};

use crate::{
    pipeline::encoder::{AudioEncoder, AudioEncoderConfig},
    utils::AudioSamplesBuffer,
};

use crate::prelude::*;

#[derive(Debug)]
pub struct OpusEncoder {
    encoder: opus::Encoder,
    sample_rate: u32,
    input_buffer: AudioSamplesBuffer,
    output_buffer: Vec<u8>,

    // This logic relays on the fact that input samples will always be continuous.
    first_input_pts: Option<Duration>,
    encoded_samples: u64,
}

impl AudioEncoder for OpusEncoder {
    const LABEL: &'static str = "libopus encoder";

    type Options = OpusEncoderOptions;

    fn new(
        _ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, AudioEncoderConfig), EncoderInitError> {
        info!(?options, "Initializing libopus encoder");
        let mut encoder = opus::Encoder::new(
            options.sample_rate,
            options.channels.into(),
            options.preset.into(),
        )?;
        encoder.set_inband_fec(options.forward_error_correction)?;
        encoder.set_packet_loss_perc(options.packet_loss)?;

        let output_buffer = vec![0u8; 1024 * 1024];

        Ok((
            Self {
                encoder,
                sample_rate: options.sample_rate,
                input_buffer: AudioSamplesBuffer::new(options.channels),
                output_buffer,
                first_input_pts: None,
                encoded_samples: 0,
            },
            AudioEncoderConfig { extradata: None },
        ))
    }

    fn set_packet_loss(&mut self, packet_loss: i32) {
        if let Err(e) = self.encoder.set_packet_loss_perc(packet_loss) {
            error!(%e, "Error while setting opus encoder packet loss.");
        }
    }

    fn encode(&mut self, batch: OutputAudioSamples) -> Vec<EncodedOutputChunk> {
        self.first_input_pts.get_or_insert(batch.start_pts);
        trace!(?batch, "libopus encoder received samples.");
        self.input_buffer.push_back(batch.samples);
        self.inner_encode(false)
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        trace!("Flushing libopus encoder");
        self.inner_encode(true)
    }
}

impl OpusEncoder {
    fn inner_encode(&mut self, force: bool) -> Vec<EncodedOutputChunk> {
        let mut result = vec![];
        while self.input_buffer.frames() >= 960 || (force && self.input_buffer.frames() > 0) {
            let samples = self.input_buffer.read_samples(960);
            let raw_samples: Vec<_> = match samples {
                AudioSamples::Mono(samples) => samples
                    .iter()
                    .map(|val| (*val * i16::MAX as f64) as i16)
                    .collect(),
                AudioSamples::Stereo(samples) => samples
                    .iter()
                    .flat_map(|(l, r)| {
                        [(*l * i16::MAX as f64) as i16, (*r * i16::MAX as f64) as i16]
                    })
                    .collect(),
            };

            let data = match self.encoder.encode(&raw_samples, &mut self.output_buffer) {
                Ok(len) => Bytes::copy_from_slice(&self.output_buffer[..len]),
                Err(err) => {
                    error!("Opus encoding error: {}", err);
                    continue;
                }
            };

            result.push(EncodedOutputChunk {
                data,
                pts: self.first_input_pts.unwrap_or_default()
                    + Duration::from_secs_f64(
                        self.encoded_samples as f64 / self.sample_rate as f64,
                    ),
                dts: None,
                is_keyframe: false,
                kind: MediaKind::Audio(AudioCodec::Opus),
            });
            self.encoded_samples += 960;
        }
        result
    }
}

impl From<OpusEncoderPreset> for opus::Application {
    fn from(value: OpusEncoderPreset) -> Self {
        match value {
            OpusEncoderPreset::Quality => opus::Application::Audio,
            OpusEncoderPreset::Voip => opus::Application::Voip,
            OpusEncoderPreset::LowestLatency => opus::Application::LowDelay,
        }
    }
}
