use std::{sync::Arc, time::Duration};
use tracing::{debug, trace};

use crate::{
    error::InputInitError,
    pipeline::types::{EncodedChunk, Samples},
};

use super::{AudioDecoderExt, DecodedSamples, DecodingError};

pub(super) struct OpusDecoder {
    decoder: opus::Decoder,
    decoded_samples_buffer: Vec<i16>,
    decoded_sample_rate: u32,

    /// PTS if the last successfully decoded sample
    last_decoded_pts: Option<Duration>,
}

impl OpusDecoder {
    pub fn new(mixing_sample_rate: u32) -> Result<Self, InputInitError> {
        const OPUS_SAMPLE_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];
        let decoded_sample_rate = if OPUS_SAMPLE_RATES.contains(&mixing_sample_rate) {
            mixing_sample_rate
        } else {
            48_000
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
            last_decoded_pts: None,
        })
    }

    /// Panics if buffer.len() < 2 * decoded_samples_count
    fn read_buffer(buffer: &[i16], decoded_samples_count: usize) -> Arc<Samples> {
        Samples::Stereo16Bit(
            buffer[0..(2 * decoded_samples_count)]
                .chunks_exact(2)
                .map(|c| (c[0], c[1]))
                .collect(),
        )
        .into()
    }

    /// Calculates PTS of the last sample in the chunk and sets `last_decoded_pts` field to it
    fn set_end_pts(&mut self, decoded_samples: &DecodedSamples) {
        let samples_len = decoded_samples.samples.sample_count();
        let sample_rate = decoded_samples.sample_rate;

        let chunk_duration = Duration::from_secs_f64(samples_len as f64 / sample_rate as f64);
        self.last_decoded_pts = Some(decoded_samples.start_pts + chunk_duration);
    }

    fn should_use_fec(&self, stream_gap: Duration) -> bool {
        stream_gap > Duration::from_millis(1)
    }

    fn calculate_stream_gap(&mut self, current_start: Duration) -> Duration {
        let stream_gap = current_start - *self.last_decoded_pts.get_or_insert(current_start);
        trace!("Calculated stream gap {stream_gap:?}");
        stream_gap
    }

    fn calculate_fec_buf_size(&self, stream_gap: Duration) -> usize {
        // 120 samples is 2.5 ms with 48kHz sample rate. For FEC it is mandatory that buffer size
        // is a multiple of 2.5 ms and of the same size (or at least as close as possible) to the
        // size of lost chunks.
        let lost_samples = stream_gap.as_secs_f64() * self.decoded_sample_rate as f64;
        let fec_buf_size = 120 * (lost_samples / 120.0f64).round() as usize;

        2 * fec_buf_size // Multiplication by for stereo
    }

    fn decode_chunk(
        &mut self,
        encoded_chunk: &EncodedChunk,
    ) -> Result<DecodedSamples, DecodingError> {
        let decoded_samples_count =
            self.decoder
                .decode(&encoded_chunk.data, &mut self.decoded_samples_buffer, false)?;

        let samples = Self::read_buffer(&self.decoded_samples_buffer, decoded_samples_count);
        Ok(DecodedSamples {
            samples,
            start_pts: encoded_chunk.pts,
            sample_rate: self.decoded_sample_rate,
        })
    }

    fn decode_chunk_fec(
        &mut self,
        encoded_chunk: &EncodedChunk,
        stream_gap: Duration,
    ) -> Result<DecodedSamples, DecodingError> {
        debug!("FEC used!");

        let fec_buf_size = self.calculate_fec_buf_size(stream_gap);
        debug!("Expected FEC chunk size: {fec_buf_size}");

        // Because of how opus-rs implements decode function, I have to create separate
        // buffer for the code (and recreate it every time in case frames differ in size).
        // That is necessary, because opus-rs takes buffer size as length of the buffer and NOT
        // as separate argument
        let decoded_samples_count = self.decoder.decode(
            &encoded_chunk.data,
            &mut self.decoded_samples_buffer[..fec_buf_size],
            true,
        )?;
        debug!("Decoded FEC samples: {decoded_samples_count}");

        let samples = Self::read_buffer(&self.decoded_samples_buffer, decoded_samples_count);
        Ok(DecodedSamples {
            samples,
            start_pts: encoded_chunk.pts - stream_gap,
            sample_rate: self.decoded_sample_rate,
        })
    }
}

impl AudioDecoderExt for OpusDecoder {
    fn decode(
        &mut self,
        encoded_chunk: EncodedChunk,
    ) -> Result<Vec<DecodedSamples>, DecodingError> {
        let stream_gap = self.calculate_stream_gap(encoded_chunk.pts);
        let use_fec = self.should_use_fec(stream_gap);

        let fec_samples = if use_fec {
            Some(self.decode_chunk_fec(&encoded_chunk, stream_gap)?)
        } else {
            None
        };

        let decoded_samples = self.decode_chunk(&encoded_chunk)?;

        self.set_end_pts(&decoded_samples);

        match fec_samples {
            Some(samples) => Ok(vec![samples, decoded_samples]),
            None => Ok(vec![decoded_samples]),
        }
    }
}
