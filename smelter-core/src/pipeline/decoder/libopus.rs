use std::{ops::Range, sync::Arc, time::Duration};
use tracing::{debug, info, trace, warn};

use smelter_render::error::ErrorStack;

use crate::pipeline::decoder::{AudioDecoder, EncodedInputEvent};
use crate::prelude::*;

/// Upper bound on audio synthesised by PLC in one outage. Concealment decays into noise long
/// before that. The rest of a longer gap stays a gap in the timeline.
const MAX_PLC_DURATION: Duration = Duration::from_millis(120);

/// Smallest opus frame, it also limits granularity of what can be concealed
const SMALLEST_OPUS_FRAME: Duration = Duration::from_micros(2500);

pub(crate) struct OpusDecoder {
    decoder: opus::Decoder,
    decoded_samples_buffer: Vec<f32>,
    decoded_sample_rate: u32,

    /// End of the last produced batch. A chunk starting later means that audio was lost.
    last_end_pts: Option<Timestamp>,
    /// PLC produced since the last decoded chunk, limited by `MAX_PLC_DURATION`.
    plc_duration: Duration,
    /// Encoder warm-up samples (OpusHead pre-skip) at the start of the stream still to drop.
    /// Chunk timestamps are expected to already account for it, as ffmpeg does.
    samples_to_skip: usize,
    channel_mapping: StereoMapping,
}

impl AudioDecoder for OpusDecoder {
    const LABEL: &'static str = "OPUS decoder";

    type Options = OpusDecoderOptions;

    fn new(ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError> {
        info!("Initializing libopus decoder");
        const OPUS_SAMPLE_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];

        let decoded_sample_rate = match OPUS_SAMPLE_RATES.contains(&ctx.mixing_sample_rate) {
            true => ctx.mixing_sample_rate,
            false => 48_000,
        };
        let mut decoder = opus::Decoder::new(decoded_sample_rate, opus::Channels::Stereo)?;
        // Max sample rate for opus is 48kHz.
        // Usually packets contain 20ms audio chunks, but for safety we use buffer
        // that can hold >1s of 48kHz stereo audio (96k samples)
        let decoded_samples_buffer = vec![0.0; 100_000];

        let opus_head = options
            .opus_head
            .map(|data| OpusHead::parse(&data))
            .unwrap_or_default();
        let channel_mapping = opus_head.stereo_mapping()?;
        decoder.set_gain(opus_head.output_gain.into())?;

        Ok(Self {
            decoder,
            decoded_samples_buffer,
            decoded_sample_rate,
            last_end_pts: None,
            plc_duration: Duration::ZERO,
            // Pre-skip is counted in 48 kHz samples.
            samples_to_skip: opus_head.pre_skip as usize * decoded_sample_rate as usize / 48_000,
            channel_mapping,
        })
    }

    fn decode(
        &mut self,
        event: EncodedInputEvent,
    ) -> Result<Vec<InputAudioSamples>, DecodingError> {
        let encoded_chunk = match event {
            EncodedInputEvent::Chunk(chunk) => chunk,
            // Gaps are detected from chunk timestamps instead.
            EncodedInputEvent::LostData | EncodedInputEvent::AuDelimiter => return Ok(vec![]),
            EncodedInputEvent::Discontinuity => {
                self.last_end_pts = None;
                self.plc_duration = Duration::ZERO;
                if let Err(err) = self.decoder.reset_state() {
                    debug!("Failed to reset opus decoder state: {err}");
                }
                return Ok(vec![]);
            }
        };

        trace!(?encoded_chunk, "libopus decoder received a chunk.");

        let mut samples = Vec::new();
        // `get_nb_samples` fails only for an invalid packet, which also fails to decode below.
        if let Ok(samples_per_packet) = self.decoder.get_nb_samples(&encoded_chunk.data) {
            let packet_duration = self.samples_to_duration(samples_per_packet);
            // `encoded_chunk` can carry a copy of the packet before it (FEC), so the last lost
            // packet can be recovered. Anything earlier is synthesised with PLC.
            samples.extend(self.conceal_until(encoded_chunk.pts - packet_duration));
            samples.extend(self.recover_fec(&encoded_chunk, samples_per_packet));
        }

        match self.decode_chunk(&encoded_chunk) {
            // Only encoder warm-up, all of it skipped.
            Ok(batch) if batch.is_empty() => {}
            Ok(batch) => samples.push(batch),
            Err(err) if samples.is_empty() => return Err(err),
            // Concealment already moved `last_end_pts`, so its output is kept. The next chunk
            // conceals the failed one.
            Err(err) => warn!(
                "Audio decoder error: {}",
                ErrorStack::new(&err).into_string()
            ),
        }

        trace!(?samples, "libopus decoder produced samples.");
        Ok(samples)
    }

    fn flush(&mut self) -> Vec<InputAudioSamples> {
        vec![]
    }
}

impl OpusDecoder {
    fn decode_chunk(
        &mut self,
        encoded_chunk: &EncodedInputChunk,
    ) -> Result<InputAudioSamples, DecodingError> {
        let decoded_samples_count = self.decoder.decode_float(
            &encoded_chunk.data,
            &mut self.decoded_samples_buffer,
            false, // fec
        )?;
        // Skipping moves the start of the batch, but not its end.
        let skipped = usize::min(self.samples_to_skip, decoded_samples_count);
        self.samples_to_skip -= skipped;
        let start_pts = encoded_chunk.pts + self.samples_to_duration(skipped);
        let batch = self.read_buffer(skipped..decoded_samples_count, start_pts);
        self.last_end_pts = Some(batch.end_pts());
        self.plc_duration = Duration::ZERO;
        Ok(batch)
    }

    /// Conceals with PLC the gap between the last produced batch and `end_pts`. PLC continues the
    /// last decoded audio, so it is placed right after it. Past `MAX_PLC_DURATION` the rest stays
    /// a gap.
    fn conceal_until(&mut self, end_pts: Timestamp) -> Option<InputAudioSamples> {
        let last_end_pts = self.last_end_pts?;
        let gap = (end_pts - last_end_pts).to_duration_saturating();
        // Shorter gaps are timestamp jitter, and SILK can't conceal less than 10 ms anyway.
        if gap <= Duration::from_millis(10) {
            return None;
        }

        // stop concealing after `MAX_PLC_DURATION` if no chunk decoded
        let conceal_duration =
            Duration::min(gap, MAX_PLC_DURATION.saturating_sub(self.plc_duration));

        // libopus conceals only multiples of 2.5 ms.
        let conceal_duration = SMALLEST_OPUS_FRAME
            * (conceal_duration.as_secs_f64() / SMALLEST_OPUS_FRAME.as_secs_f64()).round() as u32;
        if conceal_duration.is_zero() {
            return None;
        }

        let plc_samples =
            (conceal_duration.as_secs_f64() * self.decoded_sample_rate as f64).round() as usize;

        let decoded_samples_count = match self.decoder.decode_float(
            &[],
            &mut self.decoded_samples_buffer[..2 * plc_samples],
            false, // fec
        ) {
            Ok(count) => count,
            Err(err) => {
                warn!("Opus PLC failed: {err}");
                return None;
            }
        };
        let batch = self.read_buffer(0..decoded_samples_count, last_end_pts);
        self.plc_duration += self.samples_to_duration(decoded_samples_count);
        self.last_end_pts = Some(batch.end_pts());
        debug!(?gap, "PLC used");
        Some(batch)
    }

    /// Recovers the packet lost right before `encoded_chunk` from its FEC, placed right before it.
    /// libopus falls back to PLC if the packet carries no FEC.
    fn recover_fec(
        &mut self,
        encoded_chunk: &EncodedInputChunk,
        samples_per_packet: usize,
    ) -> Option<InputAudioSamples> {
        let last_end_pts = self.last_end_pts?;
        let packet_duration = self.samples_to_duration(samples_per_packet);
        let gap = (encoded_chunk.pts - last_end_pts).to_duration_saturating();
        // Shorter than half a packet is timestamp jitter, not lost audio. A gap shorter than the
        // packet makes it overlap the previous batch.
        if gap < packet_duration / 2 {
            return None;
        }

        let decoded_samples_count = match self.decoder.decode_float(
            &encoded_chunk.data,
            &mut self.decoded_samples_buffer[..2 * samples_per_packet],
            true, // fec
        ) {
            Ok(count) => count,
            // A corrupted packet, decoding it reports the error.
            Err(err) => {
                debug!("Opus FEC failed: {err}");
                return None;
            }
        };
        let start_pts = encoded_chunk.pts - packet_duration;
        let batch = self.read_buffer(0..decoded_samples_count, start_pts);
        self.last_end_pts = Some(batch.end_pts());

        // Only SILK and hybrid packets can carry FEC, even those only when the encoder has it
        // enabled. Otherwise libopus used PLC.
        // TOC config is the top 5 bits: 0..=11 SILK, 12..=15 hybrid, 16..=31 CELT.
        let fec_possible = encoded_chunk.data.first().is_some_and(|toc| {
            let config = (toc & 0b1111_1000) >> 3;
            matches!(config, 0..=15)
        });
        debug!(?gap, fec_possible, "Lost packet recovered");
        Some(batch)
    }

    /// Stamps the `range` of decoded samples in the buffer with `start_pts`.
    fn read_buffer(&self, range: Range<usize>, start_pts: Timestamp) -> InputAudioSamples {
        let (samples, _) = self.decoded_samples_buffer[2 * range.start..2 * range.end].as_chunks();
        let samples = samples
            .iter()
            .map(|&sample| self.channel_mapping.map_samples(sample))
            .collect();
        InputAudioSamples::new(
            AudioSamples::Stereo(samples),
            start_pts,
            self.decoded_sample_rate,
        )
    }

    fn samples_to_duration(&self, samples: usize) -> Duration {
        Duration::from_secs_f64(samples as f64 / self.decoded_sample_rate as f64)
    }
}

/// Fields of the Opus ID header (RFC 7845 §5.1) that the decoder applies.
#[derive(Debug, Default)]
struct OpusHead {
    /// Encoder delay in 48 kHz samples.
    pre_skip: u16,
    channel_count: u8,
    /// Gain in dB, Q7.8.
    output_gain: i16,
    channel_mapping_family: u8,
    /// Fields below are present only for mapping families other than 0.
    stream_count: u8,
    /// Streams coding two channels, they come first.
    coupled_count: u8,
    /// Decoded channel index for each output channel.
    channel_mapping: Vec<u8>,
}

impl OpusHead {
    /// Some E-RTMP senders send a truncated header, fields it doesn't reach keep their defaults.
    fn parse(data: &[u8]) -> Self {
        if !data.starts_with(b"OpusHead") {
            warn!("Invalid Opus ID header, ignoring it.");
            return Self::default();
        }
        let le_u16 = |offset: usize| {
            data.get(offset..offset + 2)
                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        };
        let channel_count = data.get(9).copied().unwrap_or(0);
        Self {
            channel_count,
            pre_skip: le_u16(10).unwrap_or(0),
            output_gain: le_u16(16).unwrap_or(0) as i16,
            channel_mapping_family: data.get(18).copied().unwrap_or(0),
            stream_count: data.get(19).copied().unwrap_or(0),
            coupled_count: data.get(20).copied().unwrap_or(0),
            channel_mapping: data
                .get(21..21 + channel_count as usize)
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
        }
    }

    /// Mono and stereo only, more channels are coded as multiple streams.
    fn stereo_mapping(&self) -> Result<StereoMapping, DecoderInitError> {
        match self.channel_mapping_family {
            0 => Ok(StereoMapping::Normal),
            // A single coupled stereo stream is the same as family 0, only the channel order can
            // differ.
            1 if self.channel_count == 2 && self.stream_count == 1 && self.coupled_count == 1 => {
                match self.channel_mapping[..] {
                    [0, 1] => Ok(StereoMapping::Normal),
                    [1, 0] => Ok(StereoMapping::Swapped),
                    _ => Err(DecoderInitError::UnsupportedOpusChannelMappingFamily(1)),
                }
            }
            family => Err(DecoderInitError::UnsupportedOpusChannelMappingFamily(
                family,
            )),
        }
    }
}

/// Order of the decoded stereo channels in the output.
#[derive(Debug, Clone, Copy)]
enum StereoMapping {
    Normal,
    /// Stream codes the right channel first.
    Swapped,
}

impl StereoMapping {
    fn map_samples(self, [first, second]: [f32; 2]) -> (f64, f64) {
        match self {
            Self::Normal => (first as f64, second as f64),
            Self::Swapped => (second as f64, first as f64),
        }
    }
}
