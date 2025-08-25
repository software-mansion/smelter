use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhipClient {
    /// WHIP server endpoint
    pub endpoint_url: Arc<str>,
    // Bearer token
    pub bearer_token: Option<Arc<str>>,
    /// Video track configuration.
    pub video: Option<OutputWhipClientVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputWhipClientAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputWhipClientVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Defines when output stream should end if some of the input streams are finished. If output includes both audio and video streams, then EOS needs to be sent on both.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: Option<VideoEncoderOptions>,
    /// Codec preferences list.
    pub encoder_preferences: Option<Vec<WhipClientVideoEncoderOptions>>,
    /// Root of a component tree/scene that should be rendered for the output.
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipClientVideoEncoderOptions {
    #[serde(rename = "ffmpeg_h264")]
    FfmpegH264 {
        /// (**default=`"fast"`**) Preset for an encoder. See `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
        preset: Option<H264EncoderPreset>,

        /// (**default=`"yuv420p"`**) Encoder pixel format
        pixel_format: Option<PixelFormat>,

        /// Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
        ffmpeg_options: Option<HashMap<String, String>>,
    },
    #[serde(rename = "ffmpeg_vp8")]
    FfmpegVp8 {
        /// Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
        ffmpeg_options: Option<HashMap<String, String>>,
    },
    #[serde(rename = "ffmpeg_vp9")]
    FfmpegVp9 {
        /// (**default=`"yuv420p"`**) Encoder pixel format
        pixel_format: Option<PixelFormat>,

        /// Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
        ffmpeg_options: Option<HashMap<String, String>>,
    },
    #[serde(rename = "any")]
    Any,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputWhipClientAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: Option<WhipClientAudioEncoderOptions>,
    /// Specifies channels configuration.
    pub channels: Option<AudioChannels>,
    /// Codec preferences list.
    pub encoder_preferences: Option<Vec<WhipClientAudioEncoderOptions>>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipClientAudioEncoderOptions {
    Opus {
        /// Specifies channels configuration.
        channels: Option<AudioChannels>,

        /// (**default="voip"**) Specifies preset for audio output encoder.
        preset: Option<OpusEncoderPreset>,

        /// (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
        sample_rate: Option<u32>,

        /// (**default=`false`**) Specifies if forward error correction (FEC) should be used.
        forward_error_correction: Option<bool>,
    },
    Any,
}
