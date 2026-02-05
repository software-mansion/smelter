use std::{collections::HashMap, time::Duration};

mod input;
mod mix;
mod mixer;

pub(crate) use mixer::AudioMixer;

use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct InputSamplesSet {
    pub samples: HashMap<InputId, Vec<InputAudioSamples>>,
    pub start_pts: Duration,
    pub end_pts: Duration,
}

#[derive(Debug)]
pub struct OutputSamplesSet(pub HashMap<OutputId, OutputAudioSamples>);

impl From<AudioChannels> for opus::Channels {
    fn from(value: AudioChannels) -> Self {
        match value {
            AudioChannels::Mono => opus::Channels::Mono,
            AudioChannels::Stereo => opus::Channels::Stereo,
        }
    }
}

impl OutputSamplesSet {
    fn merge(&mut self, mut second_set: Self) {
        for (output_id, first) in &mut self.0 {
            if let Some(second) = second_set.0.remove(output_id) {
                first.samples.merge(second.samples);
            }
        }
    }
}
