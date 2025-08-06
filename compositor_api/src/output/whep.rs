use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhepOutput {
    /// Token used for authentication in WHEP protocol. If not provided, the random value
    /// will be generated and returned in the response.
    pub bearer_token: Option<Arc<str>>,
    /// Video track configuration.
    pub video: Option<OutputVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputWhepAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputWhepAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: WhepAudioEncoderOptions,
    /// Specifies channels configuration.
    pub channels: Option<AudioChannels>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WhepAudioEncoderOptions {
    Opus {
        /// (**default="voip"**) Specifies preset for audio output encoder.
        preset: Option<OpusEncoderPreset>,

        /// (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
        sample_rate: Option<u32>,

        /// (**default=`false`**) Specifies if forward error correction (FEC) should be used.
        forward_error_correction: Option<bool>,

        /// (**default=`0`**) Expected packet loss. When `forward_error_correction` is set to `true`,
        /// then this value should be greater than `0`. Allowed values: [0, 100];
        expected_packet_loss: Option<u32>,
    },
}
