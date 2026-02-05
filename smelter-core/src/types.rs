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

    pub(crate) fn merge(&mut self, samples: Self) {
        match (self, samples) {
            (AudioSamples::Mono(first), AudioSamples::Mono(mut second)) => {
                first.append(&mut second);
            }
            (AudioSamples::Stereo(first), AudioSamples::Stereo(mut second)) => {
                first.append(&mut second);
            }
            // Options below are clearly errors, but I think it's better to just
            // handle it.
            (AudioSamples::Mono(first), AudioSamples::Stereo(second)) => {
                let mut second_mono = second.into_iter().map(|(l, r)| (l + r) / 2.0).collect();
                first.append(&mut second_mono);
            }
            (AudioSamples::Stereo(first), AudioSamples::Mono(second)) => {
                let mut second_stereo = second.into_iter().map(|value| (value, value)).collect();
                first.append(&mut second_stereo);
            }
        }
    }
}

impl fmt::Debug for AudioSamples {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.len();
        match self {
            AudioSamples::Mono(samples) => {
                let first_samples = &samples[0..usize::min(5, len)];
                let last_samples = &samples[len.saturating_sub(5)..];
                write!(
                    f,
                    "AudioSamples::Mono(len={len}, {first_samples:?}..{last_samples:?})",
                )
            }
            AudioSamples::Stereo(samples) => {
                let first_samples = &samples[0..usize::min(5, len)];
                let last_samples = &samples[len.saturating_sub(5)..];
                write!(
                    f,
                    "AudioSamples::Stereo(len={len}, {first_samples:?}..{last_samples:?})"
                )
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

    pub fn to_unique_string(&self) -> String {
        format!("{}-{}", self.public_id, self.generated_id)
    }
}

impl<Id: fmt::Display + Clone> fmt::Display for Ref<Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        self.public_id.fmt(f)
    }
}
