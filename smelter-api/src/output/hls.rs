use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HlsOutput {
    /// Path to output HLS playlist.
    pub path: String,
    /// Number of segments kept in the playlist. When the limit is reached the oldest segment is removed.
    /// If not specified, no segments will removed.
    pub max_playlist_size: Option<usize>,
    /// Video track configuration.
    pub video: Option<OutputHlsVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputHlsAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputHlsVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Condition for termination of the output stream based on the input streams states. If output includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: HlsVideoEncoderOptions,
    /// Root of a component tree/scene that should be rendered for the output. Use [`update_output` request](../routes.md#update-output) to update this value after registration. [Learn more](../../concept/component.md).
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HlsVideoEncoderOptions {
    #[serde(rename = "ffmpeg_h264")]
    FfmpegH264 {
        /// (**default=`"fast"`**) Video output encoder preset. Visit `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
        preset: Option<H264EncoderPreset>,

        /// (**default=`"yuv420p"`**) Encoder pixel format
        pixel_format: Option<PixelFormat>,

        /// Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
        ffmpeg_options: Option<HashMap<Arc<str>, Arc<str>>>,
    },
    #[serde(rename = "vulkan_h264")]
    VulkanH264 {
        /// Encoding bitrate. If not provided, bitrate is calculated based on resolution and framerate.
        /// For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max bitrate is 6250 kbit/s.
        bitrate: Option<VideoEncoderBitrate>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputHlsAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: HlsAudioEncoderOptions,
    /// Specifies channels configuration.
    pub channels: Option<AudioChannels>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HlsAudioEncoderOptions {
    Aac {
        /// (**default=`44100`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
        sample_rate: Option<u32>,
    },
}
