use std::sync::Arc;

use tracing::{error, info};

use crate::{
    audio_mixer::{AudioChannels, AudioSamples, OutputSamples},
    error::EncoderInitError,
    pipeline::{
        types::{EncodedChunk, EncodedChunkKind, IsKeyframe},
        AudioCodec, PipelineCtx,
    },
};

use super::{AudioEncoder, AudioEncoderConfig, AudioEncoderOptionsExt, AudioEncoderPreset};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct OpusEncoderOptions {
    pub channels: AudioChannels,
    pub preset: AudioEncoderPreset,
    pub sample_rate: u32,
    pub forward_error_correction: bool,
    pub packet_loss: i32,
}

impl AudioEncoderOptionsExt for OpusEncoderOptions {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

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

    fn set_packet_loss(&mut self, packet_loss: i32) {
        match self.encoder.set_packet_loss_perc(packet_loss) {
            Ok(_) => {}
            Err(e) => {
                error!(%e, "Error while setting opus encoder packet loss.");
            }
        }
    }

    fn encode(&mut self, batch: OutputSamples) -> Vec<EncodedChunk> {
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
            Ok(len) => vec![EncodedChunk {
                data: bytes::Bytes::copy_from_slice(&self.output_buffer[..len]),
                pts: batch.start_pts,
                dts: None,
                is_keyframe: IsKeyframe::NoKeyframes,
                kind: EncodedChunkKind::Audio(AudioCodec::Opus),
            }],
            Err(err) => {
                error!("Opus encoding error: {}", err);
                vec![]
            }
        }
    }

    fn flush(&mut self) -> Vec<EncodedChunk> {
        vec![]
    }
}

impl From<AudioEncoderPreset> for opus::Application {
    fn from(value: AudioEncoderPreset) -> Self {
        match value {
            AudioEncoderPreset::Quality => opus::Application::Audio,
            AudioEncoderPreset::Voip => opus::Application::Voip,
            AudioEncoderPreset::LowestLatency => opus::Application::LowDelay,
        }
    }
}
