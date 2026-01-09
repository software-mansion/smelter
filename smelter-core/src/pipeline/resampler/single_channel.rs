use std::{fmt, time::Duration};

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use tracing::{debug, error, trace, warn};

use crate::pipeline::resampler::SAMPLE_BATCH_DURATION;

pub(super) struct ChannelResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    input_buffer: Vec<f64>,
    output_buffer: Vec<f64>,
    resampler: rubato::Async<f64>,
    first_batch_pts: Duration,
    consumed_samples: u64,
    produced_samples: u64,
}

pub(super) struct SingleChannelBatch {
    pub start_pts: Duration,
    pub samples: Vec<f64>,
}

#[cfg(debug_assertions)]
const INTERPOLATION_PARAMS: SincInterpolationParameters = SincInterpolationParameters {
    sinc_len: 128,
    f_cutoff: 0.95,
    oversampling_factor: 128,
    interpolation: SincInterpolationType::Linear,
    window: WindowFunction::Blackman2,
};

#[cfg(not(debug_assertions))]
const INTERPOLATION_PARAMS: SincInterpolationParameters = SincInterpolationParameters {
    sinc_len: 256,
    f_cutoff: 0.95,
    oversampling_factor: 128,
    interpolation: SincInterpolationType::Cubic,
    window: WindowFunction::Blackman2,
};

impl ChannelResampler {
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        first_batch_pts: Duration,
    ) -> Result<Box<Self>, rubato::ResamplerConstructionError> {
        let samples_in_batch = ((output_sample_rate as u128 * SAMPLE_BATCH_DURATION.as_nanos())
            / 1_000_000_000) as usize;

        if !(output_sample_rate as u128 * SAMPLE_BATCH_DURATION.as_nanos())
            .is_multiple_of(1_000_000_000)
        {
            warn!(
                "Resampler cannot produce exactly {SAMPLE_BATCH_DURATION:?} chunks at sample rate {output_sample_rate}."
            )
        }

        let resampler = rubato::Async::<f64>::new_sinc(
            output_sample_rate as f64 / input_sample_rate as f64,
            1.10,
            &INTERPOLATION_PARAMS,
            samples_in_batch,
            1,
            FixedAsync::Output,
        )?;

        // Input buffer is preallocated, to push input samples and fill missing samples between them.
        // Reallocation happens per every output batch, due to drain from the begging,
        // but this shouldn't have a noticeable performance impact and reduce code complexity.
        // This could be done without allocations, but it would complicate this code substantially.
        let input_buffer = Vec::new();

        // Output buffer is preallocated to avoid allocating it on every output batch.
        let output_buffer = vec![0.0; samples_in_batch];

        Ok(Box::new(Self {
            input_sample_rate,
            output_sample_rate,
            input_buffer,
            output_buffer,
            resampler,
            first_batch_pts,
            consumed_samples: 0,
            produced_samples: 0,
        }))
    }

    pub fn resample(&mut self, batch: SingleChannelBatch) -> Vec<SingleChannelBatch> {
        self.append_to_input_buffer(batch);

        let mut resampled_chunks = Vec::new();
        while self.resampler.input_frames_next() <= self.input_buffer.len() {
            let start_pts = self.output_batch_pts();

            let input_buffer_len = self.input_buffer.len();
            let output_buffer_len = self.output_buffer.len();

            let input_buffer =
                InterleavedSlice::new(&self.input_buffer, 1, input_buffer_len).unwrap();
            let mut output_buffer =
                InterleavedSlice::new_mut(&mut self.output_buffer, 1, output_buffer_len).unwrap();

            let (consumed_samples, generated_samples) =
                match self
                    .resampler
                    .process_into_buffer(&input_buffer, &mut output_buffer, None)
                {
                    Ok(result) => result,
                    Err(err) => {
                        error!("Resampling error: {err}");
                        break;
                    }
                };

            self.consumed_samples += consumed_samples as u64;
            self.input_buffer.drain(0..consumed_samples);

            self.produced_samples += generated_samples as u64;
            let chunk = Vec::from(&self.output_buffer[0..generated_samples]);

            resampled_chunks.push(SingleChannelBatch {
                start_pts,
                samples: chunk,
            });
        }

        trace!(?resampled_chunks, "FFT resampler produced samples.");
        resampled_chunks
    }

    /// Write samples to input buffer. If there is a gap between last chunk and start_timestamp
    /// fill it with zeros.
    fn append_to_input_buffer(&mut self, batch: SingleChannelBatch) {
        let input_duration = batch.start_pts.saturating_sub(self.first_batch_pts);
        let expected_samples =
            (input_duration.as_secs_f64() * self.input_sample_rate as f64) as u64;
        let actual_samples = self.consumed_samples + self.input_buffer.len() as u64;

        // it handles missing samples, but it would not work with to much samples
        const SAMPLES_COMPARE_ERROR_MARGIN: u64 = 1;
        if expected_samples > actual_samples + SAMPLES_COMPARE_ERROR_MARGIN {
            let filling_samples = expected_samples - actual_samples;
            debug!("Filling {filling_samples} missing samples in resampler");
            for _ in 0..filling_samples {
                self.input_buffer.push(0.0);
            }
        }

        self.input_buffer.extend(batch.samples);
    }

    // Calculate PTS of the next batch based on already written samples
    fn output_batch_pts(&mut self) -> Duration {
        let send_audio_duration =
            Duration::from_secs_f64(self.produced_samples as f64 / self.output_sample_rate as f64);
        self.first_batch_pts + send_audio_duration
    }
}

impl fmt::Debug for SingleChannelBatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SampleBatch")
            .field("start_pts", &self.start_pts)
            .field("samples", &format!("len={}", self.samples.len()))
            .finish()
    }
}
