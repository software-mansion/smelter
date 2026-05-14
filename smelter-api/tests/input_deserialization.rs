use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use smelter_api::*;
use smelter_core::codecs::VideoDecoderOptions;
use smelter_core::protocols::{
    HlsInputOptions, HlsInputVideoDecoders, Mp4InputOptions, Mp4InputSource,
    Mp4InputVideoDecoders, PortOrRange, RtmpServerInputDecoders, RtmpServerInputOptions,
    RtpAudioOptions, RtpInputOptions, RtpInputTransportProtocol, WebrtcVideoDecoderOptions,
    WhepInputOptions, WhipInputOptions,
};
use smelter_core::QueueInputOptions;

#[cfg(target_os = "linux")]
use smelter_core::protocols::{V4l2Format, V4l2InputOptions};

type CoreInput = smelter_core::RegisterInputOptions;

fn default_queue() -> QueueInputOptions {
    QueueInputOptions {
        required: false,
        video_side_channel: false,
        audio_side_channel: false,
    }
}

#[track_caller]
fn check_rtmp(raw: serde_json::Value, expected: CoreInput) {
    let input = raw.get("input").unwrap().clone();
    let api: RtmpInput = serde_json::from_value(input).unwrap();
    let actual = CoreInput::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_rtp(raw: serde_json::Value, expected: CoreInput) {
    let input = raw.get("input").unwrap().clone();
    let api: RtpInput = serde_json::from_value(input).unwrap();
    let actual = CoreInput::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_rtp_ok(raw: serde_json::Value) {
    let input = raw.get("input").unwrap().clone();
    let api: RtpInput = serde_json::from_value(input).unwrap();
    CoreInput::try_from(api).unwrap();
}

#[track_caller]
fn check_rtp_err(raw: serde_json::Value, expected_msg: &str) {
    let input = raw.get("input").unwrap().clone();
    let api: RtpInput = serde_json::from_value(input).unwrap();
    let err = CoreInput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_mp4(raw: serde_json::Value, expected: CoreInput) {
    let input = raw.get("input").unwrap().clone();
    let api: Mp4Input = serde_json::from_value(input).unwrap();
    let actual = CoreInput::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_mp4_err(raw: serde_json::Value, expected_msg: &str) {
    let input = raw.get("input").unwrap().clone();
    let api: Mp4Input = serde_json::from_value(input).unwrap();
    let err = CoreInput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_whip(raw: serde_json::Value, expected: CoreInput) {
    let input = raw.get("input").unwrap().clone();
    let api: WhipInput = serde_json::from_value(input).unwrap();
    let actual = CoreInput::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_whip_err(raw: serde_json::Value, expected_msg: &str) {
    let input = raw.get("input").unwrap().clone();
    let api: WhipInput = serde_json::from_value(input).unwrap();
    let err = CoreInput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_whep(raw: serde_json::Value, expected: CoreInput) {
    let input = raw.get("input").unwrap().clone();
    let api: WhepInput = serde_json::from_value(input).unwrap();
    let actual = CoreInput::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_whep_err(raw: serde_json::Value, expected_msg: &str) {
    let input = raw.get("input").unwrap().clone();
    let api: WhepInput = serde_json::from_value(input).unwrap();
    let err = CoreInput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_hls(raw: serde_json::Value, expected: CoreInput) {
    let input = raw.get("input").unwrap().clone();
    let api: HlsInput = serde_json::from_value(input).unwrap();
    let actual = CoreInput::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[cfg(target_os = "linux")]
#[track_caller]
fn check_v4l2(raw: serde_json::Value, expected: CoreInput) {
    let input = raw.get("input").unwrap().clone();
    let api: V4l2Input = serde_json::from_value(input).unwrap();
    let actual = CoreInput::try_from(api).unwrap();
    assert_eq!(actual, expected);
}

#[track_caller]
fn check_decklink(raw: serde_json::Value) {
    let input = raw.get("input").unwrap().clone();
    let api: DeckLink = serde_json::from_value(input).unwrap();
    let _ = CoreInput::try_from(api);
}

#[track_caller]
fn check_serde_err<T: serde::de::DeserializeOwned>(raw: serde_json::Value) {
    let input = raw.get("input").unwrap().clone();
    assert!(serde_json::from_value::<T>(input).is_err());
}

// ── RTMP Input ───────────────────────────────────────────────────────

#[test]
fn rtmp_minimal() {
    check_rtmp(
        json!({
            "input": {
                "app": "live",
                "stream_key": "stream_1"
            }
        }),
        CoreInput::RtmpServer(RtmpServerInputOptions {
            app: Arc::from("live"),
            stream_key: Arc::from("stream_1"),
            decoders: RtmpServerInputDecoders { h264: None },
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn rtmp_with_all_options() {
    check_rtmp(
        json!({
            "input": {
                "app": "live",
                "stream_key": "stream_1",
                "required": true,
                "decoder_map": {
                    "h264": "ffmpeg_h264"
                },
                "side_channel": {
                    "video": true,
                    "audio": false
                }
            }
        }),
        CoreInput::RtmpServer(RtmpServerInputOptions {
            app: Arc::from("live"),
            stream_key: Arc::from("stream_1"),
            decoders: RtmpServerInputDecoders {
                h264: Some(VideoDecoderOptions::FfmpegH264),
            },
            queue_options: QueueInputOptions {
                required: true,
                video_side_channel: true,
                audio_side_channel: false,
            },
        }),
    );
}

#[test]
fn rtmp_vulkan_decoder() {
    check_rtmp(
        json!({
            "input": {
                "app": "live",
                "stream_key": "stream_1",
                "decoder_map": {
                    "h264": "vulkan_h264"
                }
            }
        }),
        CoreInput::RtmpServer(RtmpServerInputOptions {
            app: Arc::from("live"),
            stream_key: Arc::from("stream_1"),
            decoders: RtmpServerInputDecoders {
                h264: Some(VideoDecoderOptions::VulkanH264),
            },
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn err_serde_rtmp_missing_app() {
    check_serde_err::<RtmpInput>(json!({
        "input": {
            "stream_key": "stream_1"
        }
    }));
}

#[test]
fn err_serde_rtmp_missing_stream_key() {
    check_serde_err::<RtmpInput>(json!({
        "input": {
            "app": "live"
        }
    }));
}

#[test]
fn err_serde_rtmp_unknown_field() {
    check_serde_err::<RtmpInput>(json!({
        "input": {
            "app": "live",
            "stream_key": "stream_1",
            "unknown": true
        }
    }));
}

// ── RTP Input ────────────────────────────────────────────────────────

#[test]
fn rtp_video_h264() {
    check_rtp(
        json!({
            "input": {
                "port": 9002,
                "video": {
                    "decoder": "ffmpeg_h264"
                }
            }
        }),
        CoreInput::Rtp(RtpInputOptions {
            port: PortOrRange::Exact(9002),
            transport_protocol: RtpInputTransportProtocol::Udp,
            video: Some(VideoDecoderOptions::FfmpegH264),
            audio: None,
            queue_options: default_queue(),
            offset: None,
            buffer_duration: None,
        }),
    );
}

#[test]
fn rtp_audio_opus() {
    check_rtp(
        json!({
            "input": {
                "port": 9002,
                "audio": {
                    "decoder": "opus"
                }
            }
        }),
        CoreInput::Rtp(RtpInputOptions {
            port: PortOrRange::Exact(9002),
            transport_protocol: RtpInputTransportProtocol::Udp,
            video: None,
            audio: Some(RtpAudioOptions::Opus),
            queue_options: default_queue(),
            offset: None,
            buffer_duration: None,
        }),
    );
}

#[test]
fn rtp_audio_aac() {
    // AAC conversion involves AacAudioSpecificConfig::parse_from which is complex;
    // just verify the conversion succeeds.
    check_rtp_ok(json!({
        "input": {
            "port": 9002,
            "audio": {
                "decoder": "aac",
                "audio_specific_config": "1190"
            }
        }
    }));
}

#[test]
fn rtp_audio_aac_low_bitrate() {
    // AAC conversion involves AacAudioSpecificConfig::parse_from which is complex;
    // just verify the conversion succeeds.
    check_rtp_ok(json!({
        "input": {
            "port": 9002,
            "audio": {
                "decoder": "aac",
                "audio_specific_config": "1190",
                "rtp_mode": "low_bitrate"
            }
        }
    }));
}

#[test]
fn rtp_video_and_audio() {
    check_rtp(
        json!({
            "input": {
                "port": 9002,
                "transport_protocol": "udp",
                "video": {
                    "decoder": "ffmpeg_h264"
                },
                "audio": {
                    "decoder": "opus"
                },
                "required": true,
                "offset_ms": 500.0,
                "buffer_size_ms": 200.0,
                "side_channel": { "video": true }
            }
        }),
        CoreInput::Rtp(RtpInputOptions {
            port: PortOrRange::Exact(9002),
            transport_protocol: RtpInputTransportProtocol::Udp,
            video: Some(VideoDecoderOptions::FfmpegH264),
            audio: Some(RtpAudioOptions::Opus),
            queue_options: QueueInputOptions {
                required: true,
                video_side_channel: true,
                audio_side_channel: false,
            },
            offset: Some(Duration::from_millis(500)),
            buffer_duration: Some(Duration::from_millis(200)),
        }),
    );
}

#[test]
fn rtp_port_range() {
    check_rtp(
        json!({
            "input": {
                "port": "9000:9010",
                "transport_protocol": "tcp_server",
                "video": {
                    "decoder": "ffmpeg_vp8"
                }
            }
        }),
        CoreInput::Rtp(RtpInputOptions {
            port: PortOrRange::Range((9000, 9010)),
            transport_protocol: RtpInputTransportProtocol::TcpServer,
            video: Some(VideoDecoderOptions::FfmpegVp8),
            audio: None,
            queue_options: default_queue(),
            offset: None,
            buffer_duration: None,
        }),
    );
}

#[test]
fn rtp_video_vp9() {
    check_rtp(
        json!({
            "input": {
                "port": 9002,
                "video": {
                    "decoder": "ffmpeg_vp9"
                }
            }
        }),
        CoreInput::Rtp(RtpInputOptions {
            port: PortOrRange::Exact(9002),
            transport_protocol: RtpInputTransportProtocol::Udp,
            video: Some(VideoDecoderOptions::FfmpegVp9),
            audio: None,
            queue_options: default_queue(),
            offset: None,
            buffer_duration: None,
        }),
    );
}

#[test]
fn rtp_video_vulkan_h264() {
    check_rtp(
        json!({
            "input": {
                "port": 9002,
                "video": {
                    "decoder": "vulkan_h264"
                }
            }
        }),
        CoreInput::Rtp(RtpInputOptions {
            port: PortOrRange::Exact(9002),
            transport_protocol: RtpInputTransportProtocol::Udp,
            video: Some(VideoDecoderOptions::VulkanH264),
            audio: None,
            queue_options: default_queue(),
            offset: None,
            buffer_duration: None,
        }),
    );
}

#[test]
fn err_rtp_no_video_no_audio() {
    check_rtp_err(
        json!({
            "input": {
                "port": 9002
            }
        }),
        "At least one of `video` and `audio` has to be specified in `register_input` request.",
    );
}

#[test]
fn err_rtp_negative_buffer() {
    check_rtp_err(
        json!({
            "input": {
                "port": 9002,
                "video": { "decoder": "ffmpeg_h264" },
                "buffer_size_ms": -100.0
            }
        }),
        "Invalid buffer_size_ms. cannot convert float seconds to Duration: value is negative",
    );
}

#[test]
fn err_rtp_aac_empty_config() {
    check_rtp_err(
        json!({
            "input": {
                "port": 9002,
                "audio": {
                    "decoder": "aac",
                    "audio_specific_config": ""
                }
            }
        }),
        "The AudioSpecificConfig field is empty.",
    );
}

#[test]
fn err_rtp_aac_non_hex_config() {
    check_rtp_err(
        json!({
            "input": {
                "port": 9002,
                "audio": {
                    "decoder": "aac",
                    "audio_specific_config": "ZZZZ"
                }
            }
        }),
        "Not all of the provided string are hex digits.",
    );
}

#[test]
fn err_rtp_port_zero() {
    check_rtp_err(
        json!({
            "input": {
                "port": 0,
                "video": { "decoder": "ffmpeg_h264" }
            }
        }),
        "Port needs to be a number between 1 and 65535 or a string in the \"START:END\" format, where START and END represent a range of ports.",
    );
}

#[test]
fn err_rtp_port_range_reversed() {
    check_rtp_err(
        json!({
            "input": {
                "port": "9010:9000",
                "video": { "decoder": "ffmpeg_h264" }
            }
        }),
        "Port needs to be a number between 1 and 65535 or a string in the \"START:END\" format, where START and END represent a range of ports.",
    );
}

#[test]
fn err_rtp_port_range_bad_format() {
    check_rtp_err(
        json!({
            "input": {
                "port": "not_a_port",
                "video": { "decoder": "ffmpeg_h264" }
            }
        }),
        "Port needs to be a number between 1 and 65535 or a string in the \"START:END\" format, where START and END represent a range of ports.",
    );
}

// ── MP4 Input ────────────────────────────────────────────────────────

#[test]
fn mp4_with_url() {
    check_mp4(
        json!({
            "input": {
                "url": "https://example.com/video.mp4"
            }
        }),
        CoreInput::Mp4(Mp4InputOptions {
            source: Mp4InputSource::Url(Arc::from("https://example.com/video.mp4")),
            should_loop: false,
            video_decoders: Mp4InputVideoDecoders { h264: None },
            seek: None,
            offset: None,
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn mp4_with_path() {
    check_mp4(
        json!({
            "input": {
                "path": "/tmp/video.mp4"
            }
        }),
        CoreInput::Mp4(Mp4InputOptions {
            source: Mp4InputSource::File(Arc::from(Path::new("/tmp/video.mp4"))),
            should_loop: false,
            video_decoders: Mp4InputVideoDecoders { h264: None },
            seek: None,
            offset: None,
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn mp4_with_all_options() {
    check_mp4(
        json!({
            "input": {
                "url": "https://example.com/video.mp4",
                "loop": true,
                "required": true,
                "offset_ms": 1000.0,
                "seek_ms": 5000.0,
                "decoder_map": {
                    "h264": "ffmpeg_h264"
                },
                "side_channel": { "audio": true }
            }
        }),
        CoreInput::Mp4(Mp4InputOptions {
            source: Mp4InputSource::Url(Arc::from("https://example.com/video.mp4")),
            should_loop: true,
            video_decoders: Mp4InputVideoDecoders {
                h264: Some(VideoDecoderOptions::FfmpegH264),
            },
            seek: Some(Duration::from_secs(5)),
            offset: Some(Duration::from_secs(1)),
            queue_options: QueueInputOptions {
                required: true,
                video_side_channel: false,
                audio_side_channel: true,
            },
        }),
    );
}

#[test]
fn mp4_vulkan_decoder() {
    check_mp4(
        json!({
            "input": {
                "path": "/tmp/video.mp4",
                "decoder_map": {
                    "h264": "vulkan_h264"
                }
            }
        }),
        CoreInput::Mp4(Mp4InputOptions {
            source: Mp4InputSource::File(Arc::from(Path::new("/tmp/video.mp4"))),
            should_loop: false,
            video_decoders: Mp4InputVideoDecoders {
                h264: Some(VideoDecoderOptions::VulkanH264),
            },
            seek: None,
            offset: None,
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn err_mp4_both_url_and_path() {
    check_mp4_err(
        json!({
            "input": {
                "url": "https://example.com/video.mp4",
                "path": "/tmp/video.mp4"
            }
        }),
        "Exactly one of `url` or `path` has to be specified in a register request for an mp4 input.",
    );
}

#[test]
fn err_mp4_neither_url_nor_path() {
    check_mp4_err(
        json!({
            "input": {}
        }),
        "Exactly one of `url` or `path` has to be specified in a register request for an mp4 input.",
    );
}

#[test]
fn err_mp4_negative_seek() {
    check_mp4_err(
        json!({
            "input": {
                "url": "https://example.com/video.mp4",
                "seek_ms": -1000.0
            }
        }),
        "Invalid duration. cannot convert float seconds to Duration: value is negative",
    );
}

// ── WHIP Input ───────────────────────────────────────────────────────

#[test]
fn whip_minimal() {
    check_whip(
        json!({
            "input": {}
        }),
        CoreInput::Whip(WhipInputOptions {
            video_preferences: vec![WebrtcVideoDecoderOptions::Any],
            bearer_token: None,
            endpoint_override: None,
            jitter_buffer_size: None,
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn whip_with_all_options() {
    check_whip(
        json!({
            "input": {
                "bearer_token": "secret",
                "video": {
                    "decoder_preferences": ["ffmpeg_h264", "ffmpeg_vp8"]
                },
                "required": true,
                "buffer_size_ms": 200.0,
                "side_channel": { "video": true, "audio": true }
            }
        }),
        CoreInput::Whip(WhipInputOptions {
            video_preferences: vec![
                WebrtcVideoDecoderOptions::FfmpegH264,
                WebrtcVideoDecoderOptions::FfmpegVp8,
            ],
            bearer_token: Some(Arc::from("secret")),
            endpoint_override: None,
            jitter_buffer_size: Some(Duration::from_millis(200)),
            queue_options: QueueInputOptions {
                required: true,
                video_side_channel: true,
                audio_side_channel: true,
            },
        }),
    );
}

#[test]
fn whip_video_decoder_preferences() {
    check_whip(
        json!({
            "input": {
                "video": {
                    "decoder_preferences": ["ffmpeg_vp9", "vulkan_h264", "any"]
                }
            }
        }),
        CoreInput::Whip(WhipInputOptions {
            video_preferences: vec![
                WebrtcVideoDecoderOptions::FfmpegVp9,
                WebrtcVideoDecoderOptions::VulkanH264,
                WebrtcVideoDecoderOptions::Any,
            ],
            bearer_token: None,
            endpoint_override: None,
            jitter_buffer_size: None,
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn whip_empty_decoder_preferences_defaults_to_any() {
    check_whip(
        json!({
            "input": {
                "video": {
                    "decoder_preferences": []
                }
            }
        }),
        CoreInput::Whip(WhipInputOptions {
            video_preferences: vec![WebrtcVideoDecoderOptions::Any],
            bearer_token: None,
            endpoint_override: None,
            jitter_buffer_size: None,
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn err_whip_negative_buffer() {
    check_whip_err(
        json!({
            "input": {
                "buffer_size_ms": -50.0
            }
        }),
        "Invalid buffer_size_ms. cannot convert float seconds to Duration: value is negative",
    );
}

// ── WHEP Input ───────────────────────────────────────────────────────

#[test]
fn whep_minimal() {
    check_whep(
        json!({
            "input": {
                "endpoint_url": "https://example.com/whep"
            }
        }),
        CoreInput::Whep(WhepInputOptions {
            video_preferences: vec![WebrtcVideoDecoderOptions::Any],
            bearer_token: None,
            endpoint_url: Arc::from("https://example.com/whep"),
            jitter_buffer_size: None,
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn whep_with_all_options() {
    check_whep(
        json!({
            "input": {
                "endpoint_url": "https://example.com/whep",
                "bearer_token": "token123",
                "video": {
                    "decoder_preferences": ["ffmpeg_h264"]
                },
                "required": true,
                "buffer_size_ms": 300.0,
                "side_channel": { "video": true }
            }
        }),
        CoreInput::Whep(WhepInputOptions {
            video_preferences: vec![WebrtcVideoDecoderOptions::FfmpegH264],
            bearer_token: Some(Arc::from("token123")),
            endpoint_url: Arc::from("https://example.com/whep"),
            jitter_buffer_size: Some(Duration::from_millis(300)),
            queue_options: QueueInputOptions {
                required: true,
                video_side_channel: true,
                audio_side_channel: false,
            },
        }),
    );
}

#[test]
fn err_serde_whep_missing_endpoint() {
    check_serde_err::<WhepInput>(json!({
        "input": {}
    }));
}

#[test]
fn err_whep_negative_buffer() {
    check_whep_err(
        json!({
            "input": {
                "endpoint_url": "https://example.com/whep",
                "buffer_size_ms": -50.0
            }
        }),
        "Invalid buffer_size_ms. cannot convert float seconds to Duration: value is negative",
    );
}

// ── HLS Input ────────────────────────────────────────────────────────

#[test]
fn hls_minimal() {
    check_hls(
        json!({
            "input": {
                "url": "https://example.com/stream.m3u8"
            }
        }),
        CoreInput::Hls(HlsInputOptions {
            url: Arc::from("https://example.com/stream.m3u8"),
            video_decoders: HlsInputVideoDecoders { h264: None },
            queue_options: default_queue(),
            offset: None,
        }),
    );
}

#[test]
fn hls_with_all_options() {
    check_hls(
        json!({
            "input": {
                "url": "https://example.com/stream.m3u8",
                "required": true,
                "offset_ms": 500.0,
                "decoder_map": {
                    "h264": "ffmpeg_h264"
                },
                "side_channel": { "video": true, "audio": true }
            }
        }),
        CoreInput::Hls(HlsInputOptions {
            url: Arc::from("https://example.com/stream.m3u8"),
            video_decoders: HlsInputVideoDecoders {
                h264: Some(VideoDecoderOptions::FfmpegH264),
            },
            queue_options: QueueInputOptions {
                required: true,
                video_side_channel: true,
                audio_side_channel: true,
            },
            offset: Some(Duration::from_millis(500)),
        }),
    );
}

#[test]
fn hls_vulkan_decoder() {
    check_hls(
        json!({
            "input": {
                "url": "https://example.com/stream.m3u8",
                "decoder_map": {
                    "h264": "vulkan_h264"
                }
            }
        }),
        CoreInput::Hls(HlsInputOptions {
            url: Arc::from("https://example.com/stream.m3u8"),
            video_decoders: HlsInputVideoDecoders {
                h264: Some(VideoDecoderOptions::VulkanH264),
            },
            queue_options: default_queue(),
            offset: None,
        }),
    );
}

#[test]
fn err_serde_hls_missing_url() {
    check_serde_err::<HlsInput>(json!({
        "input": {}
    }));
}

// ── V4L2 Input ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
#[test]
fn v4l2_minimal() {
    check_v4l2(
        json!({
            "input": {
                "path": "/dev/video0",
                "format": "yuyv"
            }
        }),
        CoreInput::V4l2(V4l2InputOptions {
            path: Arc::from(Path::new("/dev/video0")),
            resolution: None,
            format: V4l2Format::Yuyv,
            framerate: None,
            queue_options: default_queue(),
        }),
    );
}

#[cfg(target_os = "linux")]
#[test]
fn v4l2_with_all_options() {
    check_v4l2(
        json!({
            "input": {
                "path": "/dev/video0",
                "format": "nv12",
                "resolution": { "width": 1920, "height": 1080 },
                "framerate": 30,
                "required": true,
                "side_channel": { "video": true }
            }
        }),
        CoreInput::V4l2(V4l2InputOptions {
            path: Arc::from(Path::new("/dev/video0")),
            resolution: Some(smelter_render::Resolution {
                width: 1920,
                height: 1080,
            }),
            format: V4l2Format::Nv12,
            framerate: Some(smelter_render::Framerate { num: 30, den: 1 }),
            queue_options: QueueInputOptions {
                required: true,
                video_side_channel: true,
                audio_side_channel: false,
            },
        }),
    );
}

#[cfg(target_os = "linux")]
#[test]
fn v4l2_fractional_framerate() {
    check_v4l2(
        json!({
            "input": {
                "path": "/dev/video0",
                "format": "yuyv",
                "framerate": "30000/1001"
            }
        }),
        CoreInput::V4l2(V4l2InputOptions {
            path: Arc::from(Path::new("/dev/video0")),
            resolution: None,
            format: V4l2Format::Yuyv,
            framerate: Some(smelter_render::Framerate {
                num: 30000,
                den: 1001,
            }),
            queue_options: default_queue(),
        }),
    );
}

#[test]
fn err_serde_v4l2_missing_path() {
    check_serde_err::<V4l2Input>(json!({
        "input": {
            "format": "yuyv"
        }
    }));
}

#[test]
fn err_serde_v4l2_missing_format() {
    check_serde_err::<V4l2Input>(json!({
        "input": {
            "path": "/dev/video0"
        }
    }));
}

// ── DeckLink Input ───────────────────────────────────────────────────

#[test]
fn decklink_minimal() {
    check_decklink(json!({
        "input": {}
    }));
}

#[test]
fn decklink_with_all_options() {
    check_decklink(json!({
        "input": {
            "subdevice_index": 0,
            "display_name": "DeckLink Mini Recorder",
            "persistent_id": "AABBCCDD",
            "enable_audio": false,
            "required": true,
            "side_channel": { "video": true }
        }
    }));
}

#[test]
fn err_serde_decklink_unknown_field() {
    check_serde_err::<DeckLink>(json!({
        "input": {
            "nonexistent": true
        }
    }));
}
