use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

/// Parameters for an input stream from RTP source.
/// At least one of `video` and `audio` has to be defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RtpInput {
    /// UDP port or port range on which the compositor should listen for the stream.
    pub port: PortOrPortRange,
    /// Transport protocol.
    pub transport_protocol: Option<TransportProtocol>,
    /// Parameters of a video source included in the RTP stream.
    pub video: Option<InputRtpVideoOptions>,
    /// Parameters of an audio source included in the RTP stream.
    pub audio: Option<InputRtpAudioOptions>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request). If the offset is
    /// not defined then the stream will be synchronized based on the delivery time of the initial
    /// frames.
    pub offset_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AacRtpMode {
    LowBitrate,
    HighBitrate,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "decoder", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputRtpAudioOptions {
    Opus,
    Aac {
        /// AudioSpecificConfig as described in MPEG-4 part 3, section 1.6.2.1
        /// The config should be encoded as described in [RFC 3640](https://datatracker.ietf.org/doc/html/rfc3640#section-4.1).
        ///
        /// The simplest way to obtain this value when using ffmpeg to stream to the compositor is
        /// to pass the additional `-sdp_file FILENAME` option to ffmpeg. This will cause it to
        /// write out an sdp file, which will contain this field. Programs which have the ability
        /// to stream AAC to the compositor should provide this information.
        ///
        /// In MP4 files, the ASC is embedded inside the esds box (note that it is not the whole
        /// box, only a part of it). This also applies to fragmented MP4s downloaded over HLS, if
        /// the playlist uses MP4s instead of MPEG Transport Streams
        ///
        /// In FLV files and the RTMP protocol, the ASC can be found in the `AACAUDIODATA` tag.
        audio_specific_config: String,
        /// (**default=`"high_bitrate"`**)
        /// Specifies the [RFC 3640 mode](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.1)
        /// that should be used when depacketizing this stream.
        rtp_mode: Option<AacRtpMode>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputRtpVideoOptions {
    pub decoder: RtpVideoDecoderOptions,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RtpVideoDecoderOptions {
    /// Software H264 decoder based on FFmpeg.
    FfmpegH264,

    /// Software VP8 decoder based on FFmpeg.
    FfmpegVp8,

    /// Software VP9 decoder based on FFmpeg.
    FfmpegVp9,

    /// Hardware decoder. Requires GPU that supports Vulkan Video decoding.
    /// Requires vk-video feature.
    VulkanH264,
}
