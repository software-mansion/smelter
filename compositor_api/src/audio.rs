use compositor_pipeline::audio_mixer;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    /// (**default=`1.0`**) float in `[0, 1]` range representing input volume
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

impl TryFrom<AudioScene> for compositor_pipeline::audio_mixer::AudioMixingParams {
    type Error = TypeError;

    fn try_from(value: AudioScene) -> Result<Self, Self::Error> {
        let mut inputs = Vec::with_capacity(value.inputs.len());
        for input in value.inputs {
            inputs.push(input.try_into()?);
        }

        Ok(Self { inputs })
    }
}

impl TryFrom<AudioSceneInput> for compositor_pipeline::audio_mixer::InputParams {
    type Error = TypeError;

    fn try_from(value: AudioSceneInput) -> Result<Self, Self::Error> {
        if let Some(volume) = value.volume {
            if !(0.0..=1.0).contains(&volume) {
                return Err(TypeError::new("Input volume has to be in [0, 1] range."));
            }
        }
        Ok(Self {
            input_id: value.input_id.into(),
            volume: value.volume.unwrap_or(1.0),
        })
    }
}

impl From<AudioMixingStrategy> for compositor_pipeline::audio_mixer::MixingStrategy {
    fn from(value: AudioMixingStrategy) -> Self {
        match value {
            AudioMixingStrategy::SumClip => {
                compositor_pipeline::audio_mixer::MixingStrategy::SumClip
            }
            AudioMixingStrategy::SumScale => {
                compositor_pipeline::audio_mixer::MixingStrategy::SumScale
            }
        }
    }
}

impl From<AudioChannels> for audio_mixer::AudioChannels {
    fn from(value: AudioChannels) -> Self {
        match value {
            AudioChannels::Mono => audio_mixer::AudioChannels::Mono,
            AudioChannels::Stereo => audio_mixer::AudioChannels::Stereo,
        }
    }
}
