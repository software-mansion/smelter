use std::{collections::HashMap, fmt::Debug, sync::Arc, time::Duration};

use compositor_render::{InputId, OutputId};


#[derive(Debug, Clone)]
pub struct InputSamplesSet {
    pub samples: HashMap<InputId, Vec<InputSamples>>,
    pub start_pts: Duration,
    pub end_pts: Duration,
}

#[derive(Debug)]
pub struct OutputSamplesSet(pub HashMap<OutputId, OutputSamples>);

#[derive(Clone)]
pub struct InputSamples {
    pub samples: Arc<Vec<(i16, i16)>>,
    pub start_pts: Duration,
    pub end_pts: Duration,
}

#[derive(Debug)]
pub struct OutputSamples {
    pub samples: AudioSamples,
    pub start_pts: Duration,
}

#[derive(Clone)]
pub enum AudioSamples {
    Mono(Vec<i16>),
    Stereo(Vec<(i16, i16)>),
}

impl InputSamplesSet {
    pub fn duration(&self) -> Duration {
        self.end_pts.saturating_sub(self.start_pts)
    }
}

impl InputSamples {
    pub fn new(
        samples: Arc<Vec<(i16, i16)>>,
        start_pts: Duration,
        mixing_sample_rate: u32,
    ) -> Self {
        let end_pts =
            start_pts + Duration::from_secs_f64(samples.len() as f64 / mixing_sample_rate as f64);

        Self {
            samples,
            start_pts,
            end_pts,
        }
    }

    pub fn duration(&self) -> Duration {
        self.end_pts.saturating_sub(self.start_pts)
    }

    pub(crate) fn len(&self) -> usize {
        self.samples.len()
    }
}

impl AudioSamples {
    pub fn len(&self) -> usize {
        match self {
            AudioSamples::Mono(samples) => samples.len(),
            AudioSamples::Stereo(samples) => samples.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Debug for InputSamples {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputSamples")
            .field("samples", &format!("len={}", self.samples.len()))
            .field("start_pts", &self.start_pts)
            .field("end_pts", &self.end_pts)
            .finish()
    }
}

impl Debug for AudioSamples {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioSamples::Mono(samples) => write!(f, "AudioSamples::Mono(len={})", samples.len()),
            AudioSamples::Stereo(samples) => {
                write!(f, "AudioSamples::Stereo(len={})", samples.len())
            }
        }
    }
}

impl From<AudioChannels> for opus::Channels {
    fn from(value: AudioChannels) -> Self {
        match value {
            AudioChannels::Mono => opus::Channels::Mono,
            AudioChannels::Stereo => opus::Channels::Stereo,
        }
    }
}
