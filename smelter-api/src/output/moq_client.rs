use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MoqClientOutput {
    /// URL of the MoQ relay to connect to. Must use the `https://` scheme.
    pub endpoint_url: Arc<str>,
    /// Path the broadcast will be published under on the relay.
    pub broadcast_path: Arc<str>,
    /// Container used to frame encoded media. Defaults to `"cmaf"`.
    pub container: Option<MoqOutputContainer>,
    /// Parameters of a video track included in the broadcast.
    pub video: Option<OutputMoqClientVideoOptions>,
    /// Parameters of an audio track included in the broadcast.
    pub audio: Option<OutputMoqClientAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MoqOutputContainer {
    /// Microsecond timestamp prefix followed by the raw codec payload.
    Legacy,
    /// Fragmented MP4. Each frame is a complete `moof`+`mdat` fragment.
    Cmaf,
    /// Low Overhead Container. (draft-ietf-moq-loc)
    Loc,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputMoqClientVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Condition for termination of the output stream based on the input streams states. If output
    /// includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: MoqClientVideoEncoderOptions,
    /// Root of a component tree/scene that should be rendered for the output. Use the
    /// `POST /api/output/{output_id}/update` request to update this value after registration.
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MoqClientVideoEncoderOptions {
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

        /// Raw FFmpeg encoder options. Visit [docs](https://ffmpeg.org/ffmpeg-codecs.html) to learn
        /// more.
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

        /// Raw FFmpeg encoder options. Visit [docs](https://ffmpeg.org/ffmpeg-codecs.html) to learn
        /// more.
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

        /// Raw FFmpeg encoder options. Visit [docs](https://ffmpeg.org/ffmpeg-codecs.html) to learn
        /// more.
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
pub struct OutputMoqClientAudioOptions {
    /// Specifies how audio should be mixed. Defaults to `"sum_clip"`.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states. If output
    /// includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: MoqClientAudioEncoderOptions,
    /// Channels configuration.
    pub channels: Option<AudioChannels>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MoqClientAudioEncoderOptions {
    Aac {
        /// Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000]. Defaults to `44100`.
        sample_rate: Option<u32>,
    },
    Opus {
        /// Audio output encoder preset. Defaults to `"voip"`.
        preset: Option<OpusEncoderPreset>,

        /// Sample rate. Allowed values: [8000, 16000, 24000, 48000]. Defaults to `48000`.
        sample_rate: Option<u32>,

        /// Specifies if forward error correction (FEC) should be used. Defaults to `false`.
        forward_error_correction: Option<bool>,

        /// Expected packet loss. When `forward_error_correction` is set to `true`, then this value
        /// should be greater than `0`. Allowed values: [0, 100];
        ///
        /// Defaults to `0`.
        expected_packet_loss: Option<u32>,
    },
}
