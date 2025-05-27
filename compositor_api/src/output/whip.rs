use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhipOutput {
    /// WHIP server endpoint
    pub endpoint_url: String,
    // Bearer token
    pub bearer_token: Option<Arc<str>>,
    /// Video track configuration.
    pub video: Option<OutputWhipVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputWhipAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputWhipVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    pub pixel_format: Option<PixelFormat>,
    /// Defines when output stream should end if some of the input streams are finished. If output includes both audio and video streams, then EOS needs to be sent on both.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: Option<VideoEncoderOptions>,
    /// Codec preferences list.
    pub encoder_preferences: Option<Vec<WhipVideoEncoderOptions>>,
    /// Root of a component tree/scene that should be rendered for the output.
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputWhipAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: Option<WhipAudioEncoderOptions>,
    /// Specifies channels configuration.
    pub channels: Option<AudioChannels>,
    /// Codec preferences list.
    pub encoder_preferences: Option<Vec<WhipAudioEncoderOptions>>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipAudioEncoderOptions {
    Opus {
        /// Specifies channels configuration.
        channels: Option<AudioChannels>,

        /// (**default="voip"**) Specifies preset for audio output encoder.
        preset: Option<OpusEncoderPreset>,

        /// (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
        sample_rate: Option<u32>,
    },
    Any,
}
