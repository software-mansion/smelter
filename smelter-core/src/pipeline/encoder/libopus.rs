use std::{sync::Arc, time::Duration};

use audioadapter::Adapter;
use bytes::Bytes;
use tracing::{error, info, trace};

use crate::{
    pipeline::encoder::{AudioEncoder, AudioEncoderConfig},
    utils::AudioSamplesBuffer,
};

use crate::prelude::*;

/// Input pts further than that from the pts derived from sample count is a discontinuity.
const MAX_PTS_DEVIATION: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct OpusEncoder {
    encoder: opus::Encoder,
    sample_rate: u32,
    /// Samples per channel in a single 20 ms Opus frame.
    frame_size: usize,
    input_buffer: AudioSamplesBuffer,
    output_buffer: Vec<u8>,
    /// Encoder lookahead in samples per channel, the delay it adds to its output.
    lookahead: usize,

    /// This logic relies on the fact that input samples will always be continuous.
    /// `maybe_reset_on_discontinuity` is just a sanity check to make sure we can recover.
    first_input_pts: Option<Timestamp>,
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

        let lookahead = encoder.get_lookahead()? as u32;
        let pre_skip = (lookahead * 48_000 / options.sample_rate) as u16;
        let extradata = opus_head(options.channels, options.sample_rate, pre_skip);
        // 20 ms, all Opus sample rates are divisible by 50.
        let frame_size = options.sample_rate / 50;

        Ok((
            Self {
                encoder,
                sample_rate: options.sample_rate,
                frame_size: frame_size as usize,
                input_buffer: AudioSamplesBuffer::new(options.channels),
                output_buffer: vec![0u8; 1024 * 1024],
                lookahead: lookahead as usize,
                first_input_pts: None,
                encoded_samples: 0,
            },
            AudioEncoderConfig {
                extradata: Some(extradata),
                initial_padding: Some(Duration::from_secs_f64(
                    lookahead as f64 / options.sample_rate as f64,
                )),
                samples_per_frame: frame_size,
            },
        ))
    }

    fn set_packet_loss(&mut self, packet_loss: i32) {
        if let Err(e) = self.encoder.set_packet_loss_perc(packet_loss) {
            error!(%e, "Error while setting opus encoder packet loss.");
        }
    }

    fn encode(&mut self, batch: OutputAudioSamples) -> Vec<EncodedOutputChunk> {
        trace!(?batch, "libopus encoder received samples.");
        self.maybe_reset_on_discontinuity(batch.start_pts);
        self.first_input_pts.get_or_insert(batch.start_pts);
        self.input_buffer.push_back(batch.samples);

        let mut result = Vec::new();
        while self.input_buffer.frames() >= self.frame_size {
            let samples = self.input_buffer.read_samples(self.frame_size);
            result.extend(self.encode_frame(samples));
        }
        result
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        trace!("Flushing libopus encoder");
        if self.first_input_pts.is_none() {
            return Vec::new();
        }
        // Input buffer should always be smaller than frame size, but with lookahead there might
        // be 2 encode calls necessary when flushing.
        let frame_count = (self.input_buffer.frames() + self.lookahead).div_ceil(self.frame_size);
        let mut result = Vec::new();
        for _ in 0..frame_count {
            // read_samples pads with zeros if not enough in buffer
            let samples = self.input_buffer.read_samples(self.frame_size);
            result.extend(self.encode_frame(samples));
        }
        result
    }
}

impl OpusEncoder {
    // Audio mixer should always produce continuous stream, just a sanity check
    fn maybe_reset_on_discontinuity(&mut self, start_pts: Timestamp) {
        let Some(first_input_pts) = self.first_input_pts else {
            return;
        };
        let samples = self.encoded_samples + self.input_buffer.frames() as u64;
        let expected_pts =
            first_input_pts + Duration::from_secs_f64(samples as f64 / self.sample_rate as f64);
        let diff_pts = start_pts - expected_pts;
        if diff_pts.abs_duration() <= MAX_PTS_DEVIATION {
            return;
        }
        error!(?diff_pts, "Discontinuity in libopus encoder input.");
        self.input_buffer.drain_samples(self.input_buffer.frames());
        self.first_input_pts = None;
        self.encoded_samples = 0;
    }

    fn encode_frame(&mut self, samples: AudioSamples) -> Option<EncodedOutputChunk> {
        let samples: Vec<f32> = match samples {
            AudioSamples::Mono(samples) => samples.iter().map(|&val| val as f32).collect(),
            AudioSamples::Stereo(samples) => samples
                .iter()
                .flat_map(|&(l, r)| [l as f32, r as f32])
                .collect(),
        };

        let first_pts = self.first_input_pts.unwrap_or_default();
        // Shifted back by lookahead, so the first sample after pre-skip has the input pts.
        let offset = Duration::from_secs_f64(self.encoded_samples as f64 / self.sample_rate as f64);
        let lookahead = Duration::from_secs_f64(self.lookahead as f64 / self.sample_rate as f64);
        let pts = first_pts + offset - lookahead;
        // Advanced even if encoding fails, samples were already consumed.
        self.encoded_samples += self.frame_size as u64;

        let data = match self.encoder.encode_float(&samples, &mut self.output_buffer) {
            Ok(len) => Bytes::copy_from_slice(&self.output_buffer[..len]),
            Err(err) => {
                error!(%err, "Opus encoding error");
                return None;
            }
        };

        Some(EncodedOutputChunk {
            data,
            pts,
            dts: None,
            is_keyframe: true,
            kind: MediaKind::Audio(AudioCodec::Opus),
        })
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
