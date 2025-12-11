use std::{collections::HashMap, time::Duration};

mod input;
mod mix;
mod mixer;

pub(crate) use mixer::AudioMixer;

use crate::prelude::*;

pub(crate) const SAMPLE_BATCH_DURATION: Duration = Duration::from_millis(20);

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
