use std::{sync::Arc, time::Duration};

use audioadapter::Adapter;
use bytes::Bytes;
use tracing::{error, info, trace};

use crate::{
    pipeline::encoder::{AudioEncoder, AudioEncoderConfig},
    utils::AudioSamplesBuffer,
};

use crate::prelude::*;

const SAMPLES_PER_BATCH: usize = 960;

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

        // OpusHead pre_skip is in 48 kHz samples (RFC 7845 §4.2) but
        // `get_lookahead` returns input-rate samples; scale or decoders miss
        // part of the pre-roll on sub-48 kHz streams. Matches ffmpeg's
        // `libavcodec/libopusenc.c:100`.
        let pre_skip = (encoder.get_lookahead()? as u32 * 48_000 / options.sample_rate) as u16;
        let extradata = opus_head(options.channels, options.sample_rate, pre_skip);

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
            AudioEncoderConfig {
                extradata: Some(extradata),
            },
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
        while self.input_buffer.frames() >= SAMPLES_PER_BATCH
            || (force && self.input_buffer.frames() > 0)
        {
            let samples = self.input_buffer.read_samples(SAMPLES_PER_BATCH);
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
                    error!(%err, "Opus encoding error");
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
            self.encoded_samples += SAMPLES_PER_BATCH as u64;
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

// RFC 7845 §5.1 OpusHead (mono/stereo, channel mapping family 0).
fn opus_head(channels: AudioChannels, sample_rate: u32, pre_skip: u16) -> Bytes {
    let channel_count: u8 = match channels {
        AudioChannels::Mono => 1,
        AudioChannels::Stereo => 2,
    };
    let mut buf = [0u8; 19];
    buf[0..8].copy_from_slice(b"OpusHead");
    buf[8] = 1;
    buf[9] = channel_count;
    buf[10..12].copy_from_slice(&pre_skip.to_le_bytes());
    buf[12..16].copy_from_slice(&sample_rate.to_le_bytes());
    buf[16..18].copy_from_slice(&0i16.to_le_bytes());
    buf[18] = 0;
    Bytes::copy_from_slice(&buf)
}
