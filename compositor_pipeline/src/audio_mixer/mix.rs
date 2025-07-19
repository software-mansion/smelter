use std::collections::HashMap;

use compositor_render::InputId;

use crate::audio_mixer::{InputParams, MixingStrategy};

use super::{
    types::{AudioChannels, AudioSamples},
    OutputInfo,
};

use tracing::{error, trace};

#[derive(Debug)]
pub(super) struct SampleMixer {
    /// Factor by which sample value is multiplied
    scaling_factor: f64,

    /// When max audio sample is above this value scaling factor is decreased
    vol_down_threshold: f64,

    /// When max audio sample is below this value scaling factor is increased
    vol_up_threshold: f64,

    /// Increment value when decreasing scaling factor
    vol_down_increment: f64,

    /// Increment value when increasing scaling factor
    vol_up_increment: f64,
}

impl SampleMixer {
    pub fn new(
        vol_down_threshold: f64,
        vol_up_threshold: f64,
        vol_down_increment: f64,
        vol_up_increment: f64,
    ) -> Self {
        Self {
            scaling_factor: 1.0,
            vol_down_threshold,
            vol_up_threshold,
            vol_down_increment,
            vol_up_increment,
        }
    }

    /// Mix input samples accordingly to provided specification.
    pub fn mix_samples(
        &mut self,
        input_samples: &HashMap<InputId, Vec<(f64, f64)>>,
        output_info: &OutputInfo,
        samples_count: usize,
    ) -> AudioSamples {
        let summed_samples = self.sum_samples(
            input_samples,
            samples_count,
            output_info.audio.inputs.iter(),
        );

        let mixed = match output_info.mixing_strategy {
            MixingStrategy::SumClip => self.clip_samples(summed_samples),
            MixingStrategy::SumScale => self.scale_samples(summed_samples),
        };

        match output_info.channels {
            AudioChannels::Mono => {
                AudioSamples::Mono(mixed.into_iter().map(|(l, r)| ((l + r) / 2.0)).collect())
            }
            AudioChannels::Stereo => AudioSamples::Stereo(mixed),
        }
    }

    fn clip_samples(&self, summed_samples: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
        summed_samples
            .into_iter()
            .map(|(l, r)| (l.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0)))
            .collect()
    }

    fn scale_samples(&mut self, summed_samples: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
        // Assumes that summed samples is not empty (therefore unwrap is safe)
        let max_sample = summed_samples
            .iter()
            .map(|(l, r)| f64::max(l.abs(), r.abs()))
            .reduce(f64::max)
            .unwrap_or_else(|| {
                error!("Mixer received an empty chunk! (This MUST NOT happen)");
                self.vol_up_threshold
            });

        let should_decrease_volume = max_sample * self.scaling_factor > self.vol_down_threshold;
        let should_increase_volume = max_sample * self.scaling_factor < self.vol_up_threshold;

        let old_scaling_factor = self.scaling_factor;
        if should_decrease_volume {
            self.scaling_factor = f64::max(self.scaling_factor - self.vol_down_increment, 0.0)
        } else if should_increase_volume {
            self.scaling_factor = f64::min(self.scaling_factor + self.vol_up_increment, 1.0)
        };
        trace!(
            max_sample,
            old_scaling_factor,
            new_scaling_factor = self.scaling_factor,
            "Processing audio sample",
        );

        let factor_diff = self.scaling_factor - old_scaling_factor;
        let sample_count = summed_samples.len();
        summed_samples
            .into_iter()
            .enumerate()
            .map(|(index, (l, r))| {
                let factor = old_scaling_factor + factor_diff * index as f64 / sample_count as f64;
                ((l * factor).clamp(-1.0, 1.0), (r * factor).clamp(-1.0, 1.0))
            })
            .collect()
    }

    /// Sums samples from inputs
    fn sum_samples<'a, I: Iterator<Item = &'a InputParams>>(
        &self,
        input_samples: &HashMap<InputId, Vec<(f64, f64)>>,
        samples_count: usize,
        inputs: I,
    ) -> Vec<(f64, f64)> {
        let mut summed_samples = vec![(0.0, 0.0); samples_count];

        for input_params in inputs {
            let Some(input_samples) = input_samples.get(&input_params.input_id) else {
                continue;
            };
            for (sum, sample) in summed_samples.iter_mut().zip(input_samples.iter()) {
                sum.0 += sample.0 * input_params.volume as f64;
                sum.1 += sample.1 * input_params.volume as f64;
            }
        }

        summed_samples
    }
}

#[cfg(test)]
mod mixer_tests;
