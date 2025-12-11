use crate::prelude::InputAudioSamples;

#[derive(Debug)]
pub(super) struct AudioMixerInput {
    mixing_sample_rate: u32,
}

impl AudioMixerInput {
    pub fn new(mixing_sample_rate: u32) -> Self {
        Self { mixing_sample_rate }
    }

    pub fn next_batch(batch: InputAudioSamples) {

    }
}
