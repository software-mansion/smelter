use std::time::Duration;

use rubato::{FftFixedOut, Resampler};
use tracing::{debug, error, info, trace};

use crate::{
    audio_mixer::{AudioSamples, OutputSamples},
    error::EncoderInitError,
};

const SAMPLE_BATCH_DURATION: Duration = Duration::from_millis(20);

enum SamplesType {
    Mono,
    Stereo,
}

impl SamplesType {
    fn new(output_samples: &OutputSamples) -> Self {
        match &output_samples.samples {
            AudioSamples::Mono(_) => Self::Mono,
            AudioSamples::Stereo(_) => Self::Stereo,
        }
    }
}

pub struct OutputResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    input_buffer: [Vec<f64>; 2],
    output_buffer: [Vec<f64>; 2],
    resampler: FftFixedOut<f64>,
    first_batch_pts: Option<Duration>,
    resampler_input_samples: u64,
    resampler_output_samples: u64,
}

impl OutputResampler {
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
    ) -> Result<OutputResampler, EncoderInitError> {
        info!(
            "Initializing output stream audio resampler  input:{} output:{}",
            input_sample_rate, output_sample_rate
        );
        /// This part of pipeline use stereo
        const CHANNELS: usize = 2;
        /// Not sure what should be here, but rubato example used 2
        /// https://github.com/HEnquist/rubato/blob/master/examples/process_f64.rs#L174
        const SUB_CHUNKS: usize = 2;
        let output_batch_size =
            (output_sample_rate as f64 * SAMPLE_BATCH_DURATION.as_secs_f64()).round() as usize;

        let resampler = rubato::FftFixedOut::<f64>::new(
            input_sample_rate as usize,
            output_sample_rate as usize,
            output_batch_size,
            SUB_CHUNKS,
            CHANNELS,
        )?;

        // Input buffer is preallocated, to push input samples and fill missing samples between them.
        // Reallocation happens per every output batch, due to drain from the begging,
        // but this shouldn't have a noticeable performance impact and reduce code complexity.
        // This could be done without allocations, but it would complicate this code substantially.
        let input_buffer = [Vec::new(), Vec::new()];

        // Output buffer is preallocated to avoid allocating it on every output batch.
        let output_buffer = [vec![0.0; output_batch_size], vec![0.0; output_batch_size]];

        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            input_buffer,
            output_buffer,
            resampler,
            first_batch_pts: None,
            resampler_input_samples: 0,
            resampler_output_samples: 0,
        })
    }

    pub fn resample(&mut self, output_samples: OutputSamples) -> Vec<OutputSamples> {
        let samples_type = SamplesType::new(&output_samples);
        self.append_to_input_buffer(output_samples);

        let mut resampled = Vec::new();
        while self.resampler.input_frames_next() <= self.input_buffer[0].len() {
            let start_pts = self.output_batch_pts();

            match self.resampler.process_into_buffer(
                &self.input_buffer,
                &mut self.output_buffer,
                None,
            ) {
                Ok((used_input_samples, produced_samples)) => {
                    let samples = self.read_output_buffer(produced_samples);
                    let audio_samples = match samples_type {
                        SamplesType::Mono => AudioSamples::Mono(stereo_samples_to_mono(samples)),
                        SamplesType::Stereo => AudioSamples::Stereo(samples),
                    };
                    let input_samples = OutputSamples {
                        samples: audio_samples,
                        start_pts,
                    };

                    self.drop_input_samples(used_input_samples);
                    self.resampler_input_samples += used_input_samples as u64;
                    self.resampler_output_samples += produced_samples as u64;
                    resampled.push(input_samples);
                }
                Err(err) => {
                    error!("Resampling error: {}", err)
                }
            }
        }

        trace!(?resampled, "FFT resampler produced samples.");
        resampled
    }

    fn append_to_input_buffer(&mut self, output_samples: OutputSamples) {
        let first_batch_pts = *self.first_batch_pts.get_or_insert(output_samples.start_pts);

        let input_duration = output_samples.start_pts.saturating_sub(first_batch_pts);
        let expected_samples =
            (input_duration.as_secs_f64() * self.input_sample_rate as f64) as u64;
        let actual_samples = self.resampler_input_samples + self.input_buffer[0].len() as u64;

        const SAMPLES_COMPARE_ERROR_MARGIN: u64 = 1;
        if expected_samples > actual_samples + SAMPLES_COMPARE_ERROR_MARGIN {
            let filling_samples = expected_samples - actual_samples;
            debug!("Filling {} missing samples in resampler", filling_samples);
            for _ in 0..filling_samples {
                self.input_buffer[0].push(0.0);
                self.input_buffer[1].push(0.0);
            }
        }

        for (l, r) in iter_as_f64_stereo(&output_samples.samples) {
            self.input_buffer[0].push(l);
            self.input_buffer[1].push(r);
        }
    }

    fn read_output_buffer(&mut self, output_samples: usize) -> Vec<(i16, i16)> {
        let left_channel_iter = self.output_buffer[0][0..output_samples].iter().cloned();
        let right_channel_iter = self.output_buffer[1][0..output_samples].iter().cloned();

        left_channel_iter
            .zip(right_channel_iter)
            .map(|(l, r)| (pcm_f64_to_i16(l), pcm_f64_to_i16(r)))
            .collect()
    }

    fn drop_input_samples(&mut self, used_samples: usize) {
        self.input_buffer[0].drain(0..used_samples);
        self.input_buffer[1].drain(0..used_samples);
    }

    fn output_batch_pts(&mut self) -> Duration {
        let send_audio_duration = Duration::from_secs_f64(
            self.resampler_output_samples as f64 / self.output_sample_rate as f64,
        );
        self.first_batch_pts.unwrap() + send_audio_duration
    }
}

fn iter_as_f64_stereo(samples: &AudioSamples) -> Vec<(f64, f64)> {
    fn pcm_i16_to_f64(val: i16) -> f64 {
        val as f64 / i16::MAX as f64
    }

    match &samples {
        crate::audio_mixer::AudioSamples::Mono(samples) => samples
            .iter()
            .map(|s| (pcm_i16_to_f64(*s), pcm_i16_to_f64(*s)))
            .collect(),
        crate::audio_mixer::AudioSamples::Stereo(samples) => samples
            .iter()
            .map(|(l, r)| (pcm_i16_to_f64(*l), pcm_i16_to_f64(*r)))
            .collect(),
    }
}

fn pcm_f64_to_i16(val: f64) -> i16 {
    let mapped_to_i16_range = val * i16::MAX as f64;
    mapped_to_i16_range
        .min(i16::MAX as f64)
        .max(i16::MIN as f64) as i16
}

// in case on mono audio, left and right channels are the same
fn stereo_samples_to_mono(samples: Vec<(i16, i16)>) -> Vec<i16> {
    samples.iter().map(|(l, _)| *l).collect()
}
