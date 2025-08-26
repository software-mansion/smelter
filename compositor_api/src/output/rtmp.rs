use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RtmpOutput {
    pub url: Arc<str>,
    /// Video stream configuration.
    pub video: Option<OutputVideoOptions>,
    /// Audio stream configuration.
    pub audio: Option<OutputRtmpClientAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputRtmpClientAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: RtmpClientAudioEncoderOptions,
    /// Specifies channels configuration.
    pub channels: Option<AudioChannels>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RtmpClientAudioEncoderOptions {
    Aac {
        channels: Option<AudioChannels>,
        /// (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
        sample_rate: Option<u32>,
    },
}
