use core::f64;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::util::*;

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

/// Parameters for an input stream for WHIP server.
/// At least one of `video` and `audio` has to be defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WhipInput {
    /// Parameters of a video source included in the RTP stream.
    pub video: Option<InputWhipVideoOptions>,
    /// Parameters of an audio source included in the RTP stream.
    pub audio: Option<InputWhipAudioOptions>,
    /// (**default=`false`**) If input is required and the stream is not delivered
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request). If the offset is
    /// not defined then the stream will be synchronized based on the delivery time of the initial
    /// frames.
    pub offset_ms: Option<f64>,
}

/// Input stream from MP4 file.
/// Exactly one of `url` and `path` has to be defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mp4Input {
    /// URL of the MP4 file.
    pub url: Option<String>,
    /// Path to the MP4 file.
    pub path: Option<String>,
    /// (**default=`false`**) If input should be played in the loop. <span class="badge badge--primary">Added in v0.4.0</span>
    #[serde(rename = "loop")]
    pub should_loop: Option<bool>,
    /// (**default=`false`**) If input is required and frames are not processed
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
    /// Offset in milliseconds relative to the pipeline start (start request). If offset is
    /// not defined then stream is synchronized based on the first frames delivery time.
    pub offset_ms: Option<f64>,
    /// (**default=`ffmpeg_h264`**) The decoder to use for decoding video.
    pub video_decoder: Option<VideoDecoder>,
}

/// Capture streams from devices connected to Blackmagic DeckLink card.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeckLink {
    /// Single DeckLink device can consist of multiple sub-devices. This field defines
    /// index of sub-device that should be used.
    ///
    /// The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
    /// All of them need to match the device if they are specified. If nothing is matched, the error response
    /// will list available devices.
    pub subdevice_index: Option<u32>,

    /// Select sub-device to use based on the display name. This is the value you see in e.g.
    /// Blackmagic Media Express app. like "DeckLink Quad HDMI Recorder (3)"
    ///
    /// The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
    /// All of them need to match the device if they are specified. If nothing is matched, the error response
    /// will list available devices.
    pub display_name: Option<String>,

    /// Persistent ID of a device represented by 32-bit hex number. Each DeckLink sub-device has a separate id.
    ///
    /// The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
    /// All of them need to match the device if they are specified. If nothing is matched, the error response
    /// will list available devices.
    pub persistent_id: Option<String>,

    /// (**default=`true`**) Enable audio support.
    pub enable_audio: Option<bool>,

    /// (**default=`false`**) If input is required and frames are not processed
    /// on time, then Smelter will delay producing output frames.
    pub required: Option<bool>,
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
    Opus {
        /// (**default=`false`**) Specifies whether the stream uses forward error correction.
        /// It's specific for Opus codec.
        /// For more information, check out [RFC](https://datatracker.ietf.org/doc/html/rfc6716#section-2.1.7).
        forward_error_correction: Option<bool>,
    },

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
#[serde(tag = "decoder", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputWhipAudioOptions {
    Opus {
        /// (**default=`false`**) Specifies whether the stream uses forward error correction.
        /// It's specific for Opus codec.
        /// For more information, check out [RFC](https://datatracker.ietf.org/doc/html/rfc6716#section-2.1.7).
        forward_error_correction: Option<bool>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputRtpVideoOptions {
    pub decoder: VideoDecoder,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct InputWhipVideoOptions {
    pub decoder: Option<VideoDecoder>,
    pub decoder_preferences: Option<Vec<WhipVideoDecoder>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum VideoDecoder {
    /// Use the software h264 decoder based on ffmpeg.
    FfmpegH264,

    /// Use the software vp8 decoder based on ffmpeg.
    FfmpegVp8,

    /// Use the software vp9 decoder based on ffmpeg.
    FfmpegVp9,

    /// Use hardware decoder based on Vulkan Video.
    ///
    /// This should be faster and more scalable than teh ffmpeg decoder, if the hardware and OS
    /// support it.
    ///
    /// This requires hardware that supports Vulkan Video. Another requirement is this program has
    /// to be compiled with the `vk-video` feature enabled (enabled by default on platforms which
    /// support Vulkan, i.e. non-Apple operating systems and not the web).
    VulkanH264,

    /// Deprected
    VulkanVideo,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WhipVideoDecoder {
    /// Use the software h264 decoder based on ffmpeg.
    FfmpegH264,

    /// Use the software vp8 decoder based on ffmpeg.
    FfmpegVp8,

    /// Use the software vp9 decoder based on ffmpeg.
    FfmpegVp9,

    /// Use hardware decoder based on Vulkan Video.
    ///
    /// This should be faster and more scalable than teh ffmpeg decoder, if the hardware and OS
    /// support it.
    ///
    /// This requires hardware that supports Vulkan Video. Another requirement is this program has
    /// to be compiled with the `vk-video` feature enabled (enabled by default on platforms which
    /// support Vulkan, i.e. non-Apple operating systems and not the web).
    VulkanH264,

    /// Deprected
    VulkanVideo,

    Any,
}
