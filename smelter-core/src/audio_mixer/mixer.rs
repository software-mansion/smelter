use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use smelter_render::{OutputId, error::UpdateSceneError};
use tracing::trace;

use crate::{
    audio_mixer::{InputSamplesSet, OutputSamplesSet, input::AudioMixerInput, mix::SampleMixer},
    prelude::OutputAudioSamples,
};

use crate::prelude::*;

#[derive(Debug, Clone)]
pub(crate) struct AudioMixer(Arc<Mutex<InternalAudioMixer>>);

impl AudioMixer {
    pub fn new(mixing_sample_rate: u32) -> Self {
        Self(Arc::new(Mutex::new(InternalAudioMixer::new(
            mixing_sample_rate,
        ))))
    }

    pub fn mix_samples(&self, samples_set: InputSamplesSet) -> OutputSamplesSet {
        trace!(set=?samples_set, "Mixing samples");
        self.0.lock().unwrap().mix_samples(samples_set)
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

    pub fn mix_samples(&mut self, mut samples_set: InputSamplesSet) -> OutputSamplesSet {
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

        OutputSamplesSet(
            self.outputs
                .iter()
                .map(|(output_id, output_info)| {
                    let samples =
                        self.sample_mixer
                            .mix_samples(&input_samples, output_info, samples_count);
                    (
                        output_id.clone(),
                        OutputAudioSamples {
                            samples,
                            start_pts: samples_set.start_pts,
                        },
                    )
                })
                .collect(),
        )
    }
}

fn expected_samples_count(start: Duration, end: Duration, sample_rate: u32) -> usize {
    (end.saturating_sub(start).as_nanos() * sample_rate as u128 / 1_000_000_000) as usize
}
