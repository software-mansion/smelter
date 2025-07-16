use std::collections::HashMap;

use compositor_render::InputId;

use crate::audio_mixer::{InputParams, MixingStrategy};

use super::{
    types::{AudioChannels, AudioSamples},
    OutputInfo,
};

use tracing::trace;

// I don't know if this is a good name, correct me if I'm wrong
#[derive(Debug)]
pub(super) struct SampleMixer {
    scaling_factor: f64,
    scaling_threshold: f64,
    scaling_increment: f64,
}

impl SampleMixer {
    pub fn new(scaling_threshold: f64, scaling_increment: f64) -> Self {
        Self {
            scaling_factor: 1.0f64,
            scaling_threshold,
            scaling_increment,
        }
    }

    /// Mix input samples accordingly to provided specification.
    pub fn mix_samples(
        &mut self,
        input_samples: &HashMap<InputId, Vec<(i16, i16)>>,
        output_info: &OutputInfo,
        samples_count: usize,
    ) -> AudioSamples {
        let summed_samples = self.sum_samples(
            input_samples,
            samples_count,
            output_info.audio.inputs.iter(),
        );

        let mixed: Vec<(i16, i16)> = match output_info.mixing_strategy {
            MixingStrategy::SumClip => self.sum_clip(summed_samples),
            MixingStrategy::SumScale => self.sum_scale(summed_samples),
        };

        match output_info.channels {
            AudioChannels::Mono => AudioSamples::Mono(
                mixed
                    .into_iter()
                    // Convert to i32 to avoid additions overflows
                    .map(|(l, r)| ((l as i32 + r as i32) / 2) as i16)
                    .collect(),
            ),
            AudioChannels::Stereo => AudioSamples::Stereo(mixed),
        }
    }

    fn sum_clip(&self, summed_samples: Vec<(i64, i64)>) -> Vec<(i16, i16)> {
        summed_samples
            .into_iter()
            .map(|(l, r)| (self.clip_to_i16(l), self.clip_to_i16(r)))
            .collect()
    }

    fn sum_scale(&mut self, summed_samples: Vec<(i64, i64)>) -> Vec<(i16, i16)> {
        let summed_samples: Vec<(f64, f64)> = summed_samples
            .into_iter()
            .map(|(l, r)| (l as f64, r as f64))
            .collect();

        // Assumes that summed samples is not empty (therefore unwrap is safe)
        let max_sample = summed_samples
            .iter()
            .map(|(l, r)| f64::max(l.abs(), r.abs()))
            .reduce(f64::max)
            .expect("Assumes that summed samples is not empty");
        trace!("Max abs value: {max_sample}");

        let new_scaling_factor = if max_sample > self.scaling_threshold {
            self.scaling_factor - self.scaling_increment
        } else {
            self.scaling_factor
        };
        trace!("Old scaling factor: {}", self.scaling_factor);
        trace!("New scaling factor: {new_scaling_factor}");

        let interpolation_increment =
            (self.scaling_factor - new_scaling_factor) / summed_samples.len() as f64;
        let mut current_scaling_factor = self.scaling_factor;

        let summed_samples: Vec<(f64, f64)> = summed_samples
            .into_iter()
            .map(|(mut l, mut r)| {
                l *= current_scaling_factor;
                r *= current_scaling_factor;
                current_scaling_factor -= interpolation_increment;
                (l, r)
            })
            .collect();

        self.scaling_factor = new_scaling_factor;

        // Shuold be removed when applying refactoring changes
        // for clarity only
        let f64_to_i16 = |x: f64| x.min(i16::MAX as f64).max(i16::MIN as f64).round() as i16;

        summed_samples
            .into_iter()
            .map(|(l, r)| (f64_to_i16(l), f64_to_i16(r)))
            .collect()
    }

    /// Clips sample to i16 PCM range
    fn clip_to_i16(&self, sample: i64) -> i16 {
        sample.min(i16::MAX as i64).max(i16::MIN as i64) as i16
    }

    /// Sums samples from inputs
    fn sum_samples<'a, I: Iterator<Item = &'a InputParams>>(
        &self,
        input_samples: &HashMap<InputId, Vec<(i16, i16)>>,
        samples_count: usize,
        inputs: I,
    ) -> Vec<(i64, i64)> {
        let mut summed_samples = vec![(0i64, 0i64); samples_count];

        for input_params in inputs {
            let Some(input_samples) = input_samples.get(&input_params.input_id) else {
                continue;
            };
            for (sum, sample) in summed_samples.iter_mut().zip(input_samples.iter()) {
                sum.0 += (sample.0 as f64 * input_params.volume as f64) as i64;
                sum.1 += (sample.1 as f64 * input_params.volume as f64) as i64;
            }
        }

        summed_samples
    }
}
