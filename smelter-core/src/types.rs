use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Ref<Id: fmt::Display + Clone> {
    public_id: Id,
    generated_id: u64,
}

impl<Id: fmt::Display + Clone> Ref<Id> {
    pub fn new(input_id: &Id) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        Self {
            public_id: input_id.clone(),
            generated_id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn id(&self) -> &Id {
        &self.public_id
    }
}

impl<Id: fmt::Display + Clone> fmt::Display for Ref<Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        self.public_id.fmt(f)
    }
}
