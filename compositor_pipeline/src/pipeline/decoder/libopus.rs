use std::{sync::Arc, time::Duration};
use tracing::{debug, info, trace};

use crate::{
    audio_mixer::AudioSamples,
    error::DecoderInitError,
    pipeline::{
        decoder::{AudioDecoder, DecodingError},
        types::EncodedChunk,
        PipelineCtx,
    },
};

use super::DecodedSamples;

pub use opus::Error as LibOpusError;

pub(crate) struct OpusDecoder {
    decoder: opus::Decoder,
    decoded_samples_buffer: Vec<i16>,
    decoded_sample_rate: u32,

    /// PTS of the end of the last decoded batch
    last_decoded_pts: Option<Duration>,
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
            last_decoded_pts: None,
        })
    }

    fn decode(
        &mut self,
        encoded_chunk: EncodedChunk,
    ) -> Result<Vec<DecodedSamples>, DecodingError> {
        let stream_gap = self.calculate_stream_gap(encoded_chunk.pts);
        let use_fec = self.should_use_fec(stream_gap);

        let fec_samples = match use_fec {
            true => Some(self.decode_chunk_fec(&encoded_chunk, stream_gap)?),
            false => None,
        };

        let decoded_samples = self.decode_chunk(&encoded_chunk)?;

        self.set_end_pts(&decoded_samples);

        match fec_samples {
            Some(samples) => Ok(vec![samples, decoded_samples]),
            None => Ok(vec![decoded_samples]),
        }
    }

    fn flush(&mut self) -> Vec<DecodedSamples> {
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

    /// Calculates PTS of the last sample in the chunk and sets `last_decoded_pts` field to it
    fn set_end_pts(&mut self, decoded_samples: &DecodedSamples) {
        let samples_len = decoded_samples.samples.sample_count();
        let sample_rate = decoded_samples.sample_rate;

        let chunk_duration = Duration::from_secs_f64(samples_len as f64 / sample_rate as f64);
        self.last_decoded_pts = Some(decoded_samples.start_pts + chunk_duration);
    }

    fn should_use_fec(&self, stream_gap: Duration) -> bool {
        // If stream gap is one second or larger there it doesn't matter if FEC is used,
        // there will be a gap
        (stream_gap > Duration::from_millis(1)) && (stream_gap < Duration::from_millis(1000))
    }

    fn calculate_stream_gap(&self, current_start: Duration) -> Duration {
        let stream_gap = match self.last_decoded_pts {
            Some(pts) => current_start.saturating_sub(pts),
            None => Duration::ZERO,
        };
        trace!("Calculated stream gap {stream_gap:?}");
        stream_gap
    }

    fn calculate_fec_buf_size(&self, stream_gap: Duration) -> usize {
        // 120 samples is 2.5 ms with 48kHz sample rate. For FEC it is mandatory that buffer size
        // is a multiple of 2.5 ms and of the same size (or at least as close as possible) to the
        // size of lost chunks.
        let lost_samples = stream_gap.as_secs_f64() * self.decoded_sample_rate as f64;
        let fec_buf_size = 120 * (lost_samples / 120.0f64).round() as usize;

        2 * fec_buf_size // Multiplication by the number of channels
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
