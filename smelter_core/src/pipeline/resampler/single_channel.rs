use std::{fmt, time::Duration};

use rubato::{FftFixedOut, Resampler};
use tracing::{debug, error, trace, warn};

use crate::pipeline::resampler::SAMPLE_BATCH_DURATION;

pub(super) struct ChannelResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    input_buffer: Vec<f64>,
    output_buffer: Vec<f64>,
    resampler: FftFixedOut<f64>,
    first_batch_pts: Duration,
    consumed_samples: u64,
    produced_samples: u64,
}

pub(super) struct SingleChannelBatch {
    pub start_pts: Duration,
    pub samples: Vec<f64>,
}

impl ChannelResampler {
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        first_batch_pts: Duration,
    ) -> Result<Box<Self>, rubato::ResamplerConstructionError> {
        /// Not sure what should be here, but rubato example used 2
        /// https://github.com/HEnquist/rubato/blob/master/examples/process_f64.rs#L174
        const SUB_CHUNKS: usize = 2;
        let samples_in_batch = ((output_sample_rate as u128 * SAMPLE_BATCH_DURATION.as_nanos())
            / 1_000_000_000) as usize;

        if !(output_sample_rate as u128 * SAMPLE_BATCH_DURATION.as_nanos())
            .is_multiple_of(1_000_000_000)
        {
            warn!("Resampler cannot produce exactly {SAMPLE_BATCH_DURATION:?} chunks at sample rate {output_sample_rate}.")
        }

        let resampler = rubato::FftFixedOut::<f64>::new(
            input_sample_rate as usize,
            output_sample_rate as usize,
            samples_in_batch,
            SUB_CHUNKS,
            1,
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

            let (consumed_samples, generated_samples) = match self.resampler.process_into_buffer(
                &[&self.input_buffer],
                &mut [&mut self.output_buffer],
                None,
            ) {
                Ok(result) => result,
                Err(err) => {
                    error!("Resampling error: {}", err);
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

    // Write samples to input buffer. If there is a gap between last chunk and start_timestamp
    // fill it with zeros.
    fn append_to_input_buffer(&mut self, batch: SingleChannelBatch) {
        let input_duration = batch.start_pts.saturating_sub(self.first_batch_pts);
        let expected_samples =
            (input_duration.as_secs_f64() * self.input_sample_rate as f64) as u64;
        let actual_samples = self.consumed_samples + self.input_buffer.len() as u64;

        // it handles missing samples, but it would not work with to much samples
        const SAMPLES_COMPARE_ERROR_MARGIN: u64 = 1;
        if expected_samples > actual_samples + SAMPLES_COMPARE_ERROR_MARGIN {
            let filling_samples = expected_samples - actual_samples;
            debug!("Filling {} missing samples in resampler", filling_samples);
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
