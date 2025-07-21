use std::sync::Arc;

use log::error;
use tracing::info;

use crate::prelude::*;

use super::{AudioEncoder, AudioEncoderConfig};

#[derive(Debug)]
pub struct OpusEncoder {
    encoder: opus::Encoder,
    output_buffer: Vec<u8>,
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
                output_buffer,
            },
            AudioEncoderConfig { extradata: None },
        ))
    }

    fn encode(&mut self, batch: OutputAudioSamples) -> Vec<EncodedOutputChunk> {
        let raw_samples: Vec<_> = match batch.samples {
            AudioSamples::Mono(raw_samples) => raw_samples
                .iter()
                .map(|val| (*val * i16::MAX as f64) as i16)
                .collect(),
            AudioSamples::Stereo(stereo_samples) => stereo_samples
                .iter()
                .flat_map(|(l, r)| [(*l * i16::MAX as f64) as i16, (*r * i16::MAX as f64) as i16])
                .collect(),
        };

        match self.encoder.encode(&raw_samples, &mut self.output_buffer) {
            Ok(len) => vec![EncodedOutputChunk {
                data: bytes::Bytes::copy_from_slice(&self.output_buffer[..len]),
                pts: batch.start_pts,
                dts: None,
                is_keyframe: false,
                kind: MediaKind::Audio(AudioCodec::Opus),
            }],
            Err(err) => {
                error!("Opus encoding error: {}", err);
                vec![]
            }
        }
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        vec![]
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
