use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use smelter_render::{OutputId, error::UpdateSceneError};
use tracing::{debug, trace};

use crate::{
    audio_mixer::{InputSamplesSet, OutputSamplesSet, input::AudioMixerInput, mix::SampleMixer},
    prelude::OutputAudioSamples,
};

use crate::prelude::*;

/// Audio mixer responsible for generating output samples from input samples.
///
/// It meets following constraints:
/// - Input samples can change theirs sample rate and channel layout dynamically.
/// - Input samples set PTS, should not have any numerical error. end pts of one set is the same
///   as start_pts of the other set. There can be gaps, but they are always multiples of entire
///   sets.
/// - Each input batch is only delivered once, it's responsibility of the AudioMixer to cache
///   unused samples between calls.
/// - Input samples need to be delivered a bit earlier than samples_set.start_pts. Resampler has
///   it's own latency, so for correct synchronization we need that buffer. Additionally, we have
///   more space to stretch audio if it's falling behind real time clock.
///   - For example, if samples_set has start_pts=40ms and end_pts=60ms, then it's best that sample
///     batches for each input already contain data for e.g. 80ms. Currently, queue enforces 40ms
///     "buffer"
/// - Output samples will always be continuous. Even if mix_samples won't be called
///   for a specific range the zero output samples will be returned on output.
///   - The consequence for downstream elements (encoder, resampler) is that they can
///     ignore packet timestamps and just take into account sample count when processing data.
/// - Output sample batches can have different sizes, nothing guarantees specific batch size.
///   Encoders like libopus need to handle grouping bytes internally.
#[derive(Debug, Clone)]
pub(crate) struct AudioMixer(Arc<Mutex<InternalAudioMixer>>);

impl AudioMixer {
    pub fn new(mixing_sample_rate: u32) -> Self {
        Self(Arc::new(Mutex::new(InternalAudioMixer::new(
            mixing_sample_rate,
        ))))
    }

    pub fn process_batch_set(&self, samples_set: InputSamplesSet) -> OutputSamplesSet {
        trace!(set=?samples_set, "Mixing samples");
        self.0.lock().unwrap().process_batch_set(samples_set)
    }

    pub fn register_input(&self, input_id: InputId) {
        self.0.lock().unwrap().register_input(input_id);
    }

    pub fn register_output(
        &self,
        output_id: OutputId,
        audio: AudioMixerConfig,
        mixing_strategy: AudioMixingStrategy,
        channels: AudioChannels,
    ) {
        self.0.lock().unwrap().outputs.insert(
            output_id,
            AudioOutputInfo {
                audio,
                channels,
                mixing_strategy,
            },
        );
    }

    pub fn unregister_output(&self, output_id: &OutputId) {
        self.0.lock().unwrap().outputs.remove(output_id);
    }

    pub fn unregister_input(&self, input_id: &InputId) {
        self.0.lock().unwrap().inputs.remove(input_id);
    }

    pub fn update_output(
        &self,
        output_id: &OutputId,
        audio: AudioMixerConfig,
    ) -> Result<(), UpdateSceneError> {
        self.0.lock().unwrap().update_output(output_id, audio)
    }
}

const VOL_DOWN_THRESHOLD: f64 = 1.0;
const VOL_UP_THRESHOLD: f64 = 0.7;
const VOL_DOWN_INCREMENT: f64 = 0.02;
const VOL_UP_INCREMENT: f64 = 0.01;

#[derive(Debug)]
pub(super) struct AudioOutputInfo {
    pub audio: AudioMixerConfig,
    pub mixing_strategy: AudioMixingStrategy,
    pub channels: AudioChannels,
}

#[derive(Debug)]
pub(super) struct InternalAudioMixer {
    outputs: HashMap<OutputId, AudioOutputInfo>,
    inputs: HashMap<InputId, AudioMixerInput>,
    mixing_sample_rate: u32,
    sample_mixer: SampleMixer,
    last_processed_batch_end: Option<Duration>,
}

impl InternalAudioMixer {
    pub fn new(mixing_sample_rate: u32) -> Self {
        Self {
            outputs: HashMap::new(),
            inputs: HashMap::new(),
            mixing_sample_rate,
            sample_mixer: SampleMixer::new(
                VOL_DOWN_THRESHOLD,
                VOL_UP_THRESHOLD,
                VOL_DOWN_INCREMENT,
                VOL_UP_INCREMENT,
            ),
            last_processed_batch_end: None,
        }
    }

    pub fn register_input(&mut self, input_id: InputId) {
        self.inputs
            .insert(input_id, AudioMixerInput::new(self.mixing_sample_rate));
    }

    pub fn update_output(
        &mut self,
        output_id: &OutputId,
        audio: AudioMixerConfig,
    ) -> Result<(), UpdateSceneError> {
        match self.outputs.get_mut(output_id) {
            Some(output_info) => {
                output_info.audio = audio;
                Ok(())
            }
            None => Err(UpdateSceneError::OutputNotRegistered(output_id.clone())),
        }
    }

    pub fn process_batch_set(&mut self, mut samples_set: InputSamplesSet) -> OutputSamplesSet {
        let last_processed_batch_end = *self
            .last_processed_batch_end
            .get_or_insert(samples_set.start_pts);

        let maybe_zero_samples = if last_processed_batch_end < samples_set.start_pts {
            let missing_range = samples_set
                .start_pts
                .saturating_sub(last_processed_batch_end);
            let missing_samples =
                f64::floor(missing_range.as_secs_f64() * self.mixing_sample_rate as f64) as usize;
            debug!(?missing_samples, "Detected gap, filling with zeros");
            Some(self.mix_samples(HashMap::new(), missing_samples, last_processed_batch_end))
        } else {
            None
        };

        let pts_range = (samples_set.start_pts, samples_set.end_pts);
        for (input_id, input) in &mut self.inputs {
            if let Some(batches) = samples_set.samples.remove(input_id) {
                input.process_batch(batches, pts_range);
            } else {
                input.process_batch(vec![], pts_range);
            }
        }

        let input_samples = self
            .inputs
            .iter_mut()
            .filter_map(|(input_id, input)| {
                input
                    .get_samples(pts_range)
                    .map(|samples| (input_id.clone(), samples))
            })
            .collect();

        let samples_count = expected_samples_count(
            samples_set.start_pts,
            samples_set.end_pts,
            self.mixing_sample_rate,
        );

        let mixed_samples = self.mix_samples(input_samples, samples_count, samples_set.start_pts);

        self.last_processed_batch_end = Some(samples_set.end_pts);
        if let Some(mut samples) = maybe_zero_samples {
            samples.merge(mixed_samples);
            samples
        } else {
            mixed_samples
        }
    }

    fn mix_samples(
        &mut self,
        input_samples: HashMap<InputId, Vec<(f64, f64)>>,
        samples_count: usize,
        start_pts: Duration,
    ) -> OutputSamplesSet {
        OutputSamplesSet(
            self.outputs
                .iter()
                .map(|(output_id, output_info)| {
                    let samples =
                        self.sample_mixer
                            .mix_samples(&input_samples, output_info, samples_count);
                    (output_id.clone(), OutputAudioSamples { samples, start_pts })
                })
                .collect(),
        )
    }
}

fn expected_samples_count(start: Duration, end: Duration, sample_rate: u32) -> usize {
    (end.saturating_sub(start).as_nanos() * sample_rate as u128 / 1_000_000_000) as usize
}
