use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WhipOutput {
    /// WHIP server endpoint
    pub endpoint_url: Arc<str>,
    // Bearer token
    pub bearer_token: Option<Arc<str>>,
    /// Video track configuration.
    pub video: Option<OutputWhipVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputWhipAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputWhipVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Defines when output stream should end if some of the input streams are finished. If output
    /// includes both audio and video streams, then EOS needs to be sent on both.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Codec preferences list.
    pub encoder_preferences: Option<Vec<WhipVideoEncoderOptions>>,
    /// Root of a component tree/scene that should be rendered for the output.
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipVideoEncoderOptions {
    #[serde(rename = "ffmpeg_h264")]
    FfmpegH264 {
        /// Preset for an encoder. See `FFmpeg`
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
    #[serde(rename = "ffmpeg_vp8")]
    FfmpegVp8 {
        /// Encoding bitrate. If not provided, bitrate is calculated based on resolution and
        /// framerate. For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max
        /// bitrate is 6250 kbit/s.
        bitrate: Option<VideoEncoderBitrate>,

        /// Maximal interval between keyframes, in milliseconds. Defaults to `5000`.
        keyframe_interval_ms: Option<f64>,

        /// Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
        ffmpeg_options: Option<HashMap<Arc<str>, Arc<str>>>,
    },
    #[serde(rename = "ffmpeg_vp9")]
    FfmpegVp9 {
        /// Encoding bitrate. If not provided, bitrate is calculated based on resolution and
        /// framerate. For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max
        /// bitrate is 6250 kbit/s.
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
    #[serde(rename = "any")]
    Any,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputWhipAudioOptions {
    /// Specifies how audio should be mixed. Defaults to `"sum_clip"`.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Specifies channels configuration.
    pub channels: Option<AudioChannels>,
    /// Codec preferences list.
    pub encoder_preferences: Option<Vec<WhipAudioEncoderOptions>>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipAudioEncoderOptions {
    Opus {
        /// Specifies preset for audio output encoder. Defaults to `"voip"`.
        preset: Option<OpusEncoderPreset>,

        /// Sample rate. Allowed values: [8000, 16000, 24000, 48000]. Defaults to `48000`.
        sample_rate: Option<u32>,

        /// Specifies if forward error correction (FEC) should be used. Defaults to `false`.
        forward_error_correction: Option<bool>,
    },
    Any,
}
