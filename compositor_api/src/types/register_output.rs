use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::audio::*;
use super::util::*;
use super::video::*;
use super::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RtpOutput {
    /// Depends on the value of the `transport_protocol` field:
    ///   - `udp` - An UDP port number that RTP packets will be sent to.
    ///   - `tcp_server` - A local TCP port number or a port range that Smelter will listen for incoming connections.
    pub port: PortOrPortRange,
    /// Only valid if `transport_protocol="udp"`. IP address where RTP packets should be sent to.
    pub ip: Option<Arc<str>>,
    /// (**default=`"udp"`**) Transport layer protocol that will be used to send RTP packets.
    pub transport_protocol: Option<TransportProtocol>,
    /// Video stream configuration.
    pub video: Option<OutputVideoOptions>,
    /// Audio stream configuration.
    pub audio: Option<OutputRtpAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RtmpOutput {
    pub url: String,
    /// Video stream configuration.
    pub video: Option<OutputVideoOptions>,
    /// Audio stream configuration.
    pub audio: Option<OutputRtmpAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mp4Output {
    /// Path to output MP4 file.
    pub path: String,
    /// Video track configuration.
    pub video: Option<OutputVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputMp4AudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhipOutput {
    /// WHIP server endpoint
    pub endpoint_url: String,
    // Bearer token
    pub bearer_token: Option<Arc<str>>,
    /// Video track configuration.
    pub video: Option<OutputVideoOptions>,
    /// Audio track configuration.
    pub audio: Option<OutputWhipAudioOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputVideoOptions {
    /// Output resolution in pixels.
    pub resolution: Resolution,
    /// Defines when output stream should end if some of the input streams are finished. If output includes both audio and video streams, then EOS needs to be sent on both.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Video encoder options.
    pub encoder: VideoEncoderOptions,
    /// Root of a component tree/scene that should be rendered for the output. Use [`update_output` request](../routes.md#update-output) to update this value after registration. [Learn more](../../concept/component.md).
    pub initial: Video,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputRtpAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<MixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: RtpAudioEncoderOptions,
    /// Initial audio mixer configuration for output.
    pub initial: Audio,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputMp4AudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<MixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: Mp4AudioEncoderOptions,
    /// Initial audio mixer configuration for output.
    pub initial: Audio,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputRtmpAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<MixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: RtmpAudioEncoderOptions,
    /// Initial audio mixer configuration for output.
    pub initial: Audio,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputWhipAudioOptions {
    /// (**default="sum_clip"**) Specifies how audio should be mixed.
    pub mixing_strategy: Option<MixingStrategy>,
    /// Condition for termination of output stream based on the input streams states.
    pub send_eos_when: Option<OutputEndCondition>,
    /// Audio encoder options.
    pub encoder: WhipAudioEncoderOptions,
    /// Initial audio mixer configuration for output.
    pub initial: Audio,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum VideoEncoderOptions {
    #[serde(rename = "ffmpeg_h264")]
    FfmpegH264 {
        /// (**default=`"fast"`**) Preset for an encoder. See `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
        preset: Option<H264EncoderPreset>,

        /// Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
        ffmpeg_options: Option<HashMap<String, String>>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RtpAudioEncoderOptions {
    Opus {
        /// Specifies channels configuration.
        channels: AudioChannels,

        /// (**default="voip"**) Specifies preset for audio output encoder.
        preset: Option<OpusEncoderPreset>,

        /// (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
        sample_rate: Option<u32>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mp4AudioEncoderOptions {
    Aac {
        channels: AudioChannels,
        /// (**default=`44100`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
        sample_rate: Option<u32>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RtmpAudioEncoderOptions {
    Aac {
        channels: AudioChannels,
        /// (**default=`44100`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
        sample_rate: Option<u32>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipAudioEncoderOptions {
    Opus {
        /// Specifies channels configuration.
        channels: AudioChannels,

        /// (**default="voip"**) Specifies preset for audio output encoder.
        preset: Option<OpusEncoderPreset>,

        /// (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
        sample_rate: Option<u32>,
    },
}

/// This type defines when end of an input stream should trigger end of the output stream. Only one of those fields can be set at the time.
/// Unless specified otherwise the input stream is considered finished/ended when:
/// - TCP connection was dropped/closed.
/// - RTCP Goodbye packet (`BYE`) was received.
/// - Mp4 track has ended.
/// - Input was unregistered already (or never registered).
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OutputEndCondition {
    /// Terminate output stream if any of the input streams from the list are finished.
    pub any_of: Option<Vec<InputId>>,
    /// Terminate output stream if all the input streams from the list are finished.
    pub all_of: Option<Vec<InputId>>,
    /// Terminate output stream if any of the input streams ends. This includes streams added after the output was registered. In particular, output stream will **not be** terminated if no inputs were ever connected.
    pub any_input: Option<bool>,
    /// Terminate output stream if all the input streams finish. In particular, output stream will **be** terminated if no inputs were ever connected.
    pub all_inputs: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum H264EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
    Placebo,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpusEncoderPreset {
    /// Best for broadcast/high-fidelity application where the decoded audio
    /// should be as close as possible to the input.
    Quality,
    /// Best for most VoIP/videoconference applications where listening quality
    /// and intelligibility matter most.
    Voip,
    /// Only use when lowest-achievable latency is what matters most.
    LowestLatency,
}
