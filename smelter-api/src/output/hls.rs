use std::{collections::HashMap, path::Path, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HlsOutput {
    /// Path to output HLS playlist.
    #[schema(value_type = str)]
    pub path: Arc<Path>,
    /// Number of segments kept in the playlist. When the limit is reached the oldest segment is
    /// removed. If not specified, no segments will removed.
    pub max_playlist_size: Option<usize>,
    /// Video track configuration.
    pub video: Option<OutputHlsVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputHlsAudioOptions>,
    /// Raw FFmpeg muxer options. See [docs](https://ffmpeg.org/ffmpeg-formats.html) for more.
    /// Note: keys here may override defaults, including `hls_list_size` derived from
    /// `max_playlist_size`.
    pub ffmpeg_options: Option<HashMap<Arc<str>, Arc<str>>>,
    /// Time in milliseconds when this output should start producing data. Value `0` represents
    /// time of the start request. Output is always created when this request is handled (e.g.
    /// playlist is created), only the moment it starts receiving frames/samples is delayed.
    pub start_at_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputHlsVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Condition for termination of the output stream based on the input streams states. If output
    /// includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: HlsVideoEncoderOptions,
    /// Root of a component tree/scene that should be rendered for the output. Use the
    /// `POST /api/output/{output_id}/update` request to update this value after registration.
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HlsVideoEncoderOptions {
    #[serde(rename = "ffmpeg_h264")]
    FfmpegH264 {
        /// Video output encoder preset. Visit `FFmpeg`
        /// [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
        ///
        /// Defaults to `"fast"`.
        preset: Option<H264EncoderPreset>,

        /// Encoding bitrate. Default value depends on chosen encoder.
        bitrate: Option<VideoEncoderBitrate>,

        /// Maximal interval between keyframes, in milliseconds. Defaults to `5000`.
        keyframe_interval_ms: Option<f64>,

        /// Encoder pixel format. Defaults to `"yuv420p"`.
        pixel_format: Option<PixelFormat>,

        /// Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
        ffmpeg_options: Option<HashMap<Arc<str>, Arc<str>>>,
    },
    #[serde(rename = "vulkan_h264")]
    VulkanH264 {
        /// Encoding bitrate. If not provided, bitrate is calculated based on resolution and
        /// framerate. For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max
        /// bitrate is 6250 kbit/s.
        bitrate: Option<VideoEncoderBitrate>,

        /// Interval between keyframes, in milliseconds. Defaults to `5000`.
        keyframe_interval_ms: Option<f64>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputHlsAudioOptions {
    /// Specifies how audio should be mixed. Defaults to `"sum_clip"`.
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HlsAudioEncoderOptions {
    Aac {
        /// Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000]. Defaults to `44100`.
        sample_rate: Option<u32>,
    },
}
