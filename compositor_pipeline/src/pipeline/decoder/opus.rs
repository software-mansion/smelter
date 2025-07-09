use std::sync::Arc;

use crate::{
    error::{DecoderInitError, InputInitError},
    pipeline::{
        decoder::{audio::DecodingError, AudioDecoder},
        types::{EncodedChunk, Samples},
    },
};

use super::DecodedSamples;

pub(crate) struct OpusDecoder {
    decoder: opus::Decoder,
    decoded_samples_buffer: Vec<i16>,
    forward_error_correction: bool,
    decoded_sample_rate: u32,
}

impl AudioDecoder for OpusDecoder {
    const LABEL: &'static str = "OPUS decoder";

    type Options = ();

    fn new(
        ctx: &Arc<crate::pipeline::PipelineCtx>,
        options: Self::Options,
    ) -> Result<Self, DecoderInitError> {
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
            forward_error_correction: options.forward_error_correction,
            decoded_sample_rate,
        })
    }

    fn decode(
        &mut self,
        encoded_chunk: EncodedChunk,
    ) -> Result<Vec<DecodedSamples>, DecodingError> {
        let decoded_samples_count = self.decoder.decode(
            &encoded_chunk.data,
            &mut self.decoded_samples_buffer,
            self.forward_error_correction,
        )?;

        let samples = Self::read_buffer(&self.decoded_samples_buffer, decoded_samples_count);
        let decoded_samples = DecodedSamples {
            samples,
            start_pts: encoded_chunk.pts,
            sample_rate: self.decoded_sample_rate,
        };
        Ok(Vec::from([decoded_samples]))
    }

    fn flush(&mut self) -> Vec<InputSamples> {
        todo!()
    }
}

impl OpusDecoder {
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
}
