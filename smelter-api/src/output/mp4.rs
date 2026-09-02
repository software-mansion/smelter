use std::{collections::HashMap, path::Path, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Mp4Output {
    /// Path to output MP4 file.
    #[schema(value_type = str)]
    pub path: Arc<Path>,
    /// Video stream configuration.
    pub video: Option<OutputMp4VideoOptions>,
    /// Audio stream configuration.
    pub audio: Option<OutputMp4AudioOptions>,
    /// Raw FFmpeg muxer options. See https://ffmpeg.org/ffmpeg-formats.html for more.
    pub ffmpeg_options: Option<HashMap<Arc<str>, Arc<str>>>,
    /// Time in milliseconds when this output should start producing data. Value `0` represents
    /// time of the start request. Output is always created when this request is handled (e.g.
    /// file is created), only the moment it starts receiving frames/samples is delayed.
    pub start_at_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputMp4VideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Condition for termination of the output stream based on the input streams states. If output
    /// includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: Mp4VideoEncoderOptions,
    /// Root of a component tree/scene that should be rendered for the output. Use the
    /// `POST /api/output/{output_id}/update` request to update this value after registration.
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mp4VideoEncoderOptions {
    #[serde(rename = "ffmpeg_h264")]
    FfmpegH264 {
        /// Video output encoder preset. See https://trac.ffmpeg.org/wiki/Encode/H.264#Preset for more.
        ///
        /// Defaults to `"fast"`.
        preset: Option<H264EncoderPreset>,

        /// Encoding bitrate. Default value depends on chosen encoder.
        bitrate: Option<VideoEncoderBitrate>,

        /// Maximal interval between keyframes, in milliseconds. Defaults to `5000`.
        keyframe_interval_ms: Option<f64>,

        /// Encoder pixel format. Defaults to `"yuv420p"`.
        pixel_format: Option<PixelFormat>,

        /// Raw FFmpeg encoder options. See https://ffmpeg.org/ffmpeg-codecs.html for more.
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
pub struct OutputMp4AudioOptions {
    /// Specifies how audio should be mixed. Defaults to `"sum_clip"`.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: Mp4AudioEncoderOptions,
    /// Specifies channels configuration.
    pub channels: Option<AudioChannels>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mp4AudioEncoderOptions {
    Aac {
        /// Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000]. Defaults to `44100`.
        sample_rate: Option<u32>,
    },
}
