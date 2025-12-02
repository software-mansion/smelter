use std::{collections::HashMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RtpOutput {
    /// Depends on the value of the `transport_protocol` field:
    ///   - `udp` - An UDP port number that RTP packets will be sent to.
    ///   - `tcp_server` - A local TCP port number or a port range that Smelter will listen for incoming connections.
    pub port: PortOrPortRange,
    /// IP address to which RTP packets should be sent. This field is only valid if `transport_protocol` field is set to `udp`.
    pub ip: Option<Arc<str>>,
    /// (**default=`"udp"`**) Transport layer protocol that will be used to send RTP packets.
    pub transport_protocol: Option<TransportProtocol>,
    /// Parameters of a video included in the RTP stream.
    pub video: Option<OutputRtpVideoOptions>,
    /// Parameters of an audio included in the RTP stream.
    pub audio: Option<OutputRtpAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputRtpVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Condition for termination of the output stream based on the input streams states. If output includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: RtpVideoEncoderOptions,
    /// Root of a component tree/scene that should be rendered for the output. Use [`update_output` request](../routes.md#update-output) to update this value after registration. [Learn more](../../concept/component.md).
    pub initial: VideoScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RtpVideoEncoderOptions {
    #[serde(rename = "ffmpeg_h264")]
    FfmpegH264 {
        /// (**default=`"fast"`**) Video output encoder preset. Visit `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
        preset: Option<H264EncoderPreset>,

        /// TODO: (@jbrs) description
        bitrate: Option<VideoEncoderBitrate>,

        /// (**default=`"yuv420p"`**) Encoder pixel format.
        pixel_format: Option<PixelFormat>,

        /// Raw FFmpeg encoder options. Visit [docs](https://ffmpeg.org/ffmpeg-codecs.html) to learn more.
        ffmpeg_options: Option<HashMap<Arc<str>, Arc<str>>>,
    },
    #[serde(rename = "ffmpeg_vp8")]
    FfmpegVp8 {
        /// Encoding bitrate. If not provided, bitrate is calculated based on resolution and framerate.
        /// For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max bitrate is 6250 kbit/s.
        bitrate: Option<VideoEncoderBitrate>,

        /// Raw FFmpeg encoder options. Visit [docs](https://ffmpeg.org/ffmpeg-codecs.html) to learn more.
        ffmpeg_options: Option<HashMap<Arc<str>, Arc<str>>>,
    },
    #[serde(rename = "ffmpeg_vp9")]
    FfmpegVp9 {
        /// Encoding bitrate. If not provided, bitrate is calculated based on resolution and framerate.
        /// For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max bitrate is 6250 kbit/s.
        bitrate: Option<VideoEncoderBitrate>,

        /// (**default=`"yuv420p"`**) Encoder pixel format.
        pixel_format: Option<PixelFormat>,

        /// Raw FFmpeg encoder options. Visit [docs](https://ffmpeg.org/ffmpeg-codecs.html) to learn more.
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
pub struct OutputRtpAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<AudioMixingStrategy>,
    /// Condition for termination of output stream based on the input streams states. If output includes both audio and video streams, then EOS needs to be sent for every type.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: RtpAudioEncoderOptions,
    /// Channels configuration.
    pub channels: Option<AudioChannels>,
    /// Initial audio mixer configuration for output.
    pub initial: AudioScene,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RtpAudioEncoderOptions {
    Opus {
        /// (**default="voip"**) Audio output encoder preset.
        preset: Option<OpusEncoderPreset>,

        /// (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
        sample_rate: Option<u32>,

        /// (**default=`false`**) Specifies if forward error correction (FEC) should be used.
        forward_error_correction: Option<bool>,

        /// (**default=`0`**) Expected packet loss. When `forward_error_correction` is set to `true`,
        /// then this value should be greater than `0`. Allowed values: [0, 100];
        expected_packet_loss: Option<u32>,
    },
}
