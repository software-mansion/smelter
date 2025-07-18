use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use compositor_render::{error::UpdateSceneError, OutputId};
use tracing::trace;

mod mix;
mod prepare_inputs;
mod types;

pub use types::*;

use crate::audio_mixer::mix::SampleMixer;

use self::prepare_inputs::{expected_samples_count, prepare_input_samples};

#[derive(Debug)]
struct OutputInfo {
    audio: AudioMixingParams,
    mixing_strategy: MixingStrategy,
    channels: AudioChannels,
}

#[derive(Debug, Clone)]
pub(super) struct AudioMixer(Arc<Mutex<InternalAudioMixer>>);

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

    pub fn register_output(
        &self,
        output_id: OutputId,
        audio: AudioMixingParams,
        mixing_strategy: MixingStrategy,
        channels: AudioChannels,
    ) {
        self.0.lock().unwrap().outputs.insert(
            output_id,
            OutputInfo {
                audio,
                channels,
                mixing_strategy,
            },
        );
    }

    pub fn unregister_output(&self, output_id: &OutputId) {
        self.0.lock().unwrap().outputs.remove(output_id);
    }

    pub fn update_output(
        &self,
        output_id: &OutputId,
        audio: AudioMixingParams,
    ) -> Result<(), UpdateSceneError> {
        self.0.lock().unwrap().update_output(output_id, audio)
    }
}
const VOL_DOWN_THRESHOLD: f64 = 1.0;
const VOL_UP_THRESHOLD: f64 = 0.7;
const VOL_DOWN_INCREMENT: f64 = 0.02;
const VOL_UP_INCREMENT: f64 = 0.01;

#[derive(Debug)]
pub(super) struct InternalAudioMixer {
    outputs: HashMap<OutputId, OutputInfo>,
    mixing_sample_rate: u32,
    sample_mixer: SampleMixer,
}

impl InternalAudioMixer {
    pub fn new(mixing_sample_rate: u32) -> Self {
        Self {
            outputs: HashMap::new(),
            mixing_sample_rate,
            sample_mixer: SampleMixer::new(
                VOL_DOWN_THRESHOLD,
                VOL_UP_THRESHOLD,
                VOL_DOWN_INCREMENT,
                VOL_UP_INCREMENT,
            ),
        }
    }

    pub fn update_output(
        &mut self,
        output_id: &OutputId,
        audio: AudioMixingParams,
    ) -> Result<(), UpdateSceneError> {
        match self.outputs.get_mut(output_id) {
            Some(output_info) => {
                output_info.audio = audio;
                Ok(())
            }
            None => Err(UpdateSceneError::OutputNotRegistered(output_id.clone())),
        }
    }

    pub fn mix_samples(&mut self, samples_set: InputSamplesSet) -> OutputSamplesSet {
        let start_pts = samples_set.start_pts;
        let samples_count = expected_samples_count(
            samples_set.start_pts,
            samples_set.end_pts,
            self.mixing_sample_rate,
        );
        let input_samples = prepare_input_samples(samples_set, self.mixing_sample_rate);

        OutputSamplesSet(
            self.outputs
                .iter()
                .map(|(output_id, output_info)| {
                    let samples =
                        self.sample_mixer
                            .mix_samples(&input_samples, output_info, samples_count);
                    (output_id.clone(), OutputSamples { samples, start_pts })
                })
                .collect(),
        )
    }
}
