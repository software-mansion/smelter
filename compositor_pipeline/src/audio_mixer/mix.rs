use std::collections::HashMap;

use compositor_render::InputId;

use crate::audio_mixer::{InputParams, MixingStrategy};

use super::{
    types::{AudioChannels, AudioSamples},
    OutputInfo,
};

// I don't know if this is a good name, correct me if I'm wrong
#[derive(Debug)]
pub(super) struct SampleMixer {
    scaling_factor: f64,
}

impl SampleMixer {
    pub fn new() -> Self {
        Self {
            scaling_factor: 1.0f64,
        }
    }
    /// Mix input samples accordingly to provided specification.
    pub fn mix_samples(
        &self,
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
            MixingStrategy::SumClip => summed_samples
                .into_iter()
                .map(|(l, r)| (self.clip_to_i16(l), self.clip_to_i16(r)))
                .collect(),
            MixingStrategy::SumScale => {
                let scaling_factor = {
                    // abs panics in debug if val = i64::MIN, but it would require summing so many i16 samples, that it'll never happen.
                    // Assumes that summed samples is not empty (therefore unwrap is safe)
                    let max_abs = summed_samples
                        .iter()
                        .map(|(l, r)| i64::max(l.abs(), r.abs()))
                        .max()
                        .unwrap();
                    f64::max(max_abs as f64 / i16::MAX as f64, 1.0)
                };

                summed_samples
                    .into_iter()
                    .map(|(l, r)| {
                        (
                            self.clip_to_i16((l as f64 * scaling_factor) as i64),
                            self.clip_to_i16((r as f64 * scaling_factor) as i64),
                        )
                    })
                    .collect()
            }
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
