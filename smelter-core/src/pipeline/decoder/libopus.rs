use std::{sync::Arc, time::Duration};
use tracing::{debug, info, trace};

use crate::pipeline::decoder::{AudioDecoder, EncodedInputEvent};
use crate::prelude::*;

/// Opus's hard cap for a single decode call (see `opus_decoder.c`: `opus_int16 size[48];`,
/// 48 × 2.5 ms). We won't ask for more than this in one shot — for longer gaps the older
/// audio is dropped instead of stretched into low-quality concealment.
const MAX_DECODE_DURATION: Duration = Duration::from_millis(120);

pub(crate) struct OpusDecoder {
    decoder: opus::Decoder,
    decoded_samples_buffer: Vec<i16>,
    decoded_sample_rate: u32,

    /// Number of consecutive `LostData` events received since the last successful
    /// chunk. On the next `Chunk`, opus's FEC path reconstructs the immediately
    /// preceding frame and PLC-fills the older ones in the same call.
    unhandled_lost_packets: u32,
}

impl AudioDecoder for OpusDecoder {
    const LABEL: &'static str = "OPUS decoder";

    type Options = ();

    fn new(ctx: &Arc<PipelineCtx>, _options: Self::Options) -> Result<Self, DecoderInitError> {
        info!("Initializing libopus decoder");
        const OPUS_SAMPLE_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];

        let decoded_sample_rate = match OPUS_SAMPLE_RATES.contains(&ctx.mixing_sample_rate) {
            true => ctx.mixing_sample_rate,
            false => 48_000,
        };
        let decoder = opus::Decoder::new(decoded_sample_rate, opus::Channels::Stereo)?;
        // Max sample rate for opus is 48kHz.
        // Usually packets contain 20ms audio chunks, but for safety we use buffer
        // that can hold >1s of 48kHz stereo audio (96k samples)
        let decoded_samples_buffer = vec![0; 100_000];

        Ok(Self {
            decoder,
            decoded_samples_buffer,
            decoded_sample_rate,
            unhandled_lost_packets: 0,
        })
    }

    fn decode(
        &mut self,
        event: EncodedInputEvent,
    ) -> Result<Vec<InputAudioSamples>, DecodingError> {
        let encoded_chunk = match event {
            EncodedInputEvent::Chunk(chunk) => chunk,
            EncodedInputEvent::LostData => {
                self.unhandled_lost_packets = self.unhandled_lost_packets.saturating_add(1);
                return Ok(vec![]);
            }
            EncodedInputEvent::AuDelimiter => return Ok(vec![]),
        };

        trace!(?encoded_chunk, "libopus decoder received a chunk.");

        let recovered = match self.unhandled_lost_packets {
            0 => None,
            n => self.decode_chunk_fec(&encoded_chunk, n)?,
        };
        self.unhandled_lost_packets = 0;

        let decoded_samples = self.decode_chunk(&encoded_chunk)?;

        let samples = match recovered {
            Some(samples) => vec![samples, decoded_samples],
            None => vec![decoded_samples],
        };

        trace!(?samples, "libopus decoder produced samples.");
        Ok(samples)
    }

    fn flush(&mut self) -> Vec<InputAudioSamples> {
        vec![]
    }
}

impl OpusDecoder {
    /// Panics if buffer.len() < 2 * decoded_samples_count
    fn read_buffer(buffer: &[i16], decoded_samples_count: usize) -> AudioSamples {
        AudioSamples::Stereo(
            buffer[0..(2 * decoded_samples_count)]
                .chunks_exact(2)
                .map(|c| (c[0] as f64 / i16::MAX as f64, c[1] as f64 / i16::MAX as f64))
                .collect(),
        )
    }

    fn decode_chunk(
        &mut self,
        encoded_chunk: &EncodedInputChunk,
    ) -> Result<InputAudioSamples, DecodingError> {
        let decoded_samples_count =
            self.decoder
                .decode(&encoded_chunk.data, &mut self.decoded_samples_buffer, false)?;

        let samples = Self::read_buffer(&self.decoded_samples_buffer, decoded_samples_count);
        Ok(InputAudioSamples {
            samples,
            start_pts: encoded_chunk.pts,
            sample_rate: self.decoded_sample_rate,
        })
    }

    /// Reconstruct the run of `lost_packets` lost frames preceding `encoded_chunk`.
    ///
    /// Inside libopus's `decode_fec=1` path (see `opus_decoder.c:672–696`), only the
    /// trailing `packet_frame_size` of the requested span is real FEC — the prefix is
    /// PLC (concealment synthesised from decoder state). So this call recovers one
    /// preceding frame faithfully and fills the older losses with PLC.
    ///
    /// We assume each lost packet had the same duration as the current one; that's
    /// the convention recommended by the opus reference and holds for typical
    /// constant-duration streams.
    fn decode_chunk_fec(
        &mut self,
        encoded_chunk: &EncodedInputChunk,
        lost_packets: u32,
    ) -> Result<Option<InputAudioSamples>, DecodingError> {
        let Ok(samples_per_packet) = self.decoder.get_nb_samples(&encoded_chunk.data) else {
            debug!("Failed to read opus packet duration; skipping FEC.");
            return Ok(None);
        };
        let packet_duration =
            Duration::from_secs_f64(samples_per_packet as f64 / self.decoded_sample_rate as f64);

        // Cap how much we ask opus to synthesise. Beyond ~60–80 ms PLC degrades to
        // noise, and opus itself rejects more than 120 ms per call.
        let max_packets =
            (MAX_DECODE_DURATION.as_secs_f64() / packet_duration.as_secs_f64()) as u32;
        let recovered_packets = u32::min(lost_packets, max_packets);
        if recovered_packets == 0 {
            return Ok(None);
        }

        let samples_per_channel = samples_per_packet * recovered_packets as usize;
        let fec_buf_size = 2 * samples_per_channel;

        let decoded_samples_count = self.decoder.decode(
            &encoded_chunk.data,
            &mut self.decoded_samples_buffer[..fec_buf_size],
            true,
        )?;
        debug!(
            lost_packets,
            dropped_packets = lost_packets - recovered_packets,
            recovered_packets,
            decoded_samples_count,
            "FEC + PLC used"
        );

        // The recovered span (PLC prefix + FEC tail) is `recovered_packets` long
        // and ends immediately before the current chunk. Older dropped packets
        // stay as a gap in the timeline.
        let recovered_duration = packet_duration * recovered_packets;
        let start_pts = encoded_chunk.pts.saturating_sub(recovered_duration);

        let samples = Self::read_buffer(&self.decoded_samples_buffer, decoded_samples_count);
        Ok(Some(InputAudioSamples {
            samples,
            start_pts,
            sample_rate: self.decoded_sample_rate,
        }))
    }
}
