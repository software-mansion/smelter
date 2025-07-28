use core::fmt;

use crate::codecs::{AudioCodec, VideoCodec};

#[derive(Debug)]
pub enum PipelineEvent<T> {
    Data(T),
    EOS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Video(VideoCodec),
    Audio(AudioCodec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioChannels {
    Mono,
    Stereo,
}

#[derive(Clone)]
pub enum AudioSamples {
    Mono(Vec<f64>),
    Stereo(Vec<(f64, f64)>),
}

impl AudioSamples {
    pub fn sample_count(&self) -> usize {
        match self {
            AudioSamples::Mono(samples) => samples.len(),
            AudioSamples::Stereo(items) => items.len(),
        }
    }
}

impl fmt::Debug for AudioSamples {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioSamples::Mono(samples) => write!(f, "AudioSamples::Mono(len={})", samples.len()),
            AudioSamples::Stereo(samples) => {
                write!(f, "AudioSamples::Stereo(len={})", samples.len())
            }
        }
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
