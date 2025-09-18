use std::{collections::HashMap, fmt::Debug, time::Duration};

use smelter_render::{InputId, OutputId};

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
