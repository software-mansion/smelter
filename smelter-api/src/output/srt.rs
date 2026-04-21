use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::*;

/// Parameters for an output stream sent over SRT in listener mode.
///
/// The output is registered on Smelter's shared SRT server (configured via
/// `SMELTER_SRT_SERVER_PORT`). The output id is used as the SRT `streamid`;
/// a caller connects to that port using `streamid=<output_id>` to pull the
/// stream. While a caller is connected, further callers using the same id
/// are refused; when the active caller disconnects, the next caller is
/// accepted.
///
/// The produced stream is MPEG-TS carrying H.264 video and/or AAC audio.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SrtOutput {
    /// Video stream configuration.
    pub video: Option<OutputSrtVideoOptions>,
    /// Audio stream configuration.
    pub audio: Option<OutputSrtAudioOptions>,
    /// Enable AES encryption on the outgoing SRT stream. The caller must
    /// connect with the matching `passphrase`.
    pub encryption: Option<SrtOutputEncryption>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SrtOutputEncryption {
    /// Passphrase used to derive the AES key. Must be 10–79 characters long.
    pub passphrase: Arc<str>,
    /// AES key length used for the stream.
    pub encryption: SrtEncryption,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputSrtVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Condition for termination of the output stream based on the input streams states. If output includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: SrtVideoEncoderOptions,
    /// Root of a component tree/scene that should be rendered for the output. Use [`update_output` request](../routes.md#update-output) to update this value after registration. [Learn more](../../concept/component.md).
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SrtVideoEncoderOptions {
    #[serde(rename = "ffmpeg_h264")]
    FfmpegH264 {
        /// (**default=`"fast"`**) Video output encoder preset. Visit `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
        preset: Option<H264EncoderPreset>,

        /// Encoding bitrate. Default value depends on chosen encoder.
        bitrate: Option<VideoEncoderBitrate>,

        /// (**default=`5000`**) Maximal interval between keyframes, in milliseconds.
        keyframe_interval_ms: Option<f64>,

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

        /// (**default=`5000`**) Interval between keyframes, in milliseconds.
        keyframe_interval_ms: Option<f64>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputSrtAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of the output stream based on the input streams states. If output includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: SrtAudioEncoderOptions,
    /// Channels configuration.
    pub channels: Option<AudioChannels>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SrtAudioEncoderOptions {
    Aac {
        /// (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
        sample_rate: Option<u32>,
    },
}
