use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::common_core::prelude as core;
use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioScene {
    pub inputs: Vec<AudioSceneInput>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioSceneInput {
    pub input_id: InputId,
    /// (**default=`1.0`**) float in `[0, 2]` range representing input volume
    pub volume: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioMixingStrategy {
    /// Firstly, input samples are summed. If the result is outside the i16 PCM range, it gets clipped.
    SumClip,
    /// Firstly, input samples are summed. If the result is outside the i16 PCM range,
    /// nearby summed samples are scaled down by factor, such that the summed wave is in the i16 PCM range.
    SumScale,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioChannels {
    /// Mono audio (single channel).
    Mono,
    /// Stereo audio (two channels).
    Stereo,
}

impl TryFrom<AudioScene> for core::AudioMixerConfig {
    type Error = TypeError;

    fn try_from(value: AudioScene) -> Result<Self, Self::Error> {
        let mut inputs = Vec::with_capacity(value.inputs.len());
        for input in value.inputs {
            inputs.push(input.try_into()?);
        }

        Ok(Self { inputs })
    }
}

impl TryFrom<AudioSceneInput> for core::AudioMixerInputConfig {
    type Error = TypeError;

    fn try_from(value: AudioSceneInput) -> Result<Self, Self::Error> {
        if let Some(volume) = value.volume
            && !(0.0..=2.0).contains(&volume) {
                return Err(TypeError::new("Input volume has to be in [0, 2] range."));
            }
        Ok(Self {
            input_id: value.input_id.into(),
            volume: value.volume.unwrap_or(1.0),
        })
    }
}

impl From<AudioMixingStrategy> for core::AudioMixingStrategy {
    fn from(value: AudioMixingStrategy) -> Self {
        match value {
            AudioMixingStrategy::SumClip => core::AudioMixingStrategy::SumClip,
            AudioMixingStrategy::SumScale => core::AudioMixingStrategy::SumScale,
        }
    }
}

impl From<AudioChannels> for smelter_core::AudioChannels {
    fn from(value: AudioChannels) -> Self {
        match value {
            AudioChannels::Mono => smelter_core::AudioChannels::Mono,
            AudioChannels::Stereo => smelter_core::AudioChannels::Stereo,
        }
    }
}
