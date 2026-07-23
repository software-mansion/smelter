use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use smelter_api::*;

type CoreOutput = smelter_core::RegisterOutputOptions;

fn video_scene() -> serde_json::Value {
    json!({ "root": { "type": "view" } })
}

fn audio_scene() -> serde_json::Value {
    json!({ "inputs": [] })
}

fn default_video() -> smelter_core::RegisterOutputVideoOptions {
    smelter_core::RegisterOutputVideoOptions {
        initial: smelter_render::scene::Component::View(
            smelter_render::scene::ViewComponent::default(),
        ),
        end_condition: smelter_core::PipelineOutputEndCondition::Never,
    }
}

fn default_audio() -> smelter_core::RegisterOutputAudioOptions {
    smelter_core::RegisterOutputAudioOptions {
        initial: smelter_core::AudioMixerConfig { inputs: vec![] },
        mixing_strategy: smelter_core::AudioMixingStrategy::SumClip,
        channels: smelter_core::AudioChannels::Stereo,
        end_condition: smelter_core::PipelineOutputEndCondition::Never,
    }
}

fn default_keyframe_interval() -> Duration {
    Duration::from_millis(5000)
}

#[track_caller]
fn check_rtmp(raw: serde_json::Value, expected: CoreOutput) {
    let output = raw.get("output").unwrap().clone();
    let api: RtmpOutput = serde_json::from_value(output).unwrap();
    let result = CoreOutput::try_from(api).unwrap();
    assert_eq!(result, expected);
}

#[track_caller]
fn check_rtmp_err(raw: serde_json::Value, expected_msg: &str) {
    let output = raw.get("output").unwrap().clone();
    let api: RtmpOutput = serde_json::from_value(output).unwrap();
    let err = CoreOutput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_rtp(raw: serde_json::Value, expected: CoreOutput) {
    let output = raw.get("output").unwrap().clone();
    let api: RtpOutput = serde_json::from_value(output).unwrap();
    let result = CoreOutput::try_from(api).unwrap();
    assert_eq!(result, expected);
}

#[track_caller]
fn check_rtp_err(raw: serde_json::Value, expected_msg: &str) {
    let output = raw.get("output").unwrap().clone();
    let api: RtpOutput = serde_json::from_value(output).unwrap();
    let err = CoreOutput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_mp4(raw: serde_json::Value, expected: CoreOutput) {
    let output = raw.get("output").unwrap().clone();
    let api: Mp4Output = serde_json::from_value(output).unwrap();
    let result = CoreOutput::try_from(api).unwrap();
    assert_eq!(result, expected);
}

#[track_caller]
fn check_mp4_err(raw: serde_json::Value, expected_msg: &str) {
    let output = raw.get("output").unwrap().clone();
    let api: Mp4Output = serde_json::from_value(output).unwrap();
    let err = CoreOutput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_whip(raw: serde_json::Value, expected: CoreOutput) {
    let output = raw.get("output").unwrap().clone();
    let api: WhipOutput = serde_json::from_value(output).unwrap();
    let result = CoreOutput::try_from(api).unwrap();
    assert_eq!(result, expected);
}

#[track_caller]
fn check_whip_err(raw: serde_json::Value, expected_msg: &str) {
    let output = raw.get("output").unwrap().clone();
    let api: WhipOutput = serde_json::from_value(output).unwrap();
    let err = CoreOutput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_whep(raw: serde_json::Value, expected: CoreOutput) {
    let output = raw.get("output").unwrap().clone();
    let api: WhepOutput = serde_json::from_value(output).unwrap();
    let result = CoreOutput::try_from(api).unwrap();
    assert_eq!(result, expected);
}

#[track_caller]
fn check_whep_err(raw: serde_json::Value, expected_msg: &str) {
    let output = raw.get("output").unwrap().clone();
    let api: WhepOutput = serde_json::from_value(output).unwrap();
    let err = CoreOutput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_hls(raw: serde_json::Value, expected: CoreOutput) {
    let output = raw.get("output").unwrap().clone();
    let api: HlsOutput = serde_json::from_value(output).unwrap();
    let result = CoreOutput::try_from(api).unwrap();
    assert_eq!(result, expected);
}

#[track_caller]
fn check_hls_err(raw: serde_json::Value, expected_msg: &str) {
    let output = raw.get("output").unwrap().clone();
    let api: HlsOutput = serde_json::from_value(output).unwrap();
    let err = CoreOutput::try_from(api).unwrap_err();
    assert_eq!(err.to_string(), expected_msg);
}

#[track_caller]
fn check_moq(raw: serde_json::Value, expected: CoreOutput) {
    let output = raw.get("output").unwrap().clone();
    let api: MoqClientOutput = serde_json::from_value(output).unwrap();
    let result = CoreOutput::try_from(api).unwrap();
    assert_eq!(result, expected);
}

#[track_caller]
fn check_serde_err<T: serde::de::DeserializeOwned>(raw: serde_json::Value) {
    let output = raw.get("output").unwrap().clone();
    assert!(serde_json::from_value::<T>(output).is_err());
}

// ── RTMP Output ──────────────────────────────────────────────────────

#[test]
fn rtmp_video_only() {
    check_rtmp(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264"
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtmp(
                smelter_core::protocols::RtmpOutputOptions {
                    connection: smelter_core::protocols::RtmpConnectionOptions {
                        host: "localhost".into(),
                        port: 1935,
                        app: "live".into(),
                        stream_key: "stream".into(),
                        use_tls: false,
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::Avcc,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn rtmp_audio_only() {
    check_rtmp(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "audio": {
                    "encoder": { "type": "aac" },
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtmp(
                smelter_core::protocols::RtmpOutputOptions {
                    connection: smelter_core::protocols::RtmpConnectionOptions {
                        host: "localhost".into(),
                        port: 1935,
                        app: "live".into(),
                        stream_key: "stream".into(),
                        use_tls: false,
                    },
                    video: None,
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::FdkAac(
                        smelter_core::codecs::FdkAacEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            sample_rate: 44100,
                            bitstream_format: smelter_core::codecs::AacBitstreamFormat::Raw,
                        },
                    )),
                },
            ),
            video: None,
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn rtmp_video_and_audio() {
    check_rtmp(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1280, "height": 720 },
                    "encoder": {
                        "type": "ffmpeg_h264",
                        "preset": "ultrafast",
                        "bitrate": 4000000,
                        "keyframe_interval_ms": 2000,
                        "pixel_format": "yuv420p"
                    },
                    "initial": video_scene()
                },
                "audio": {
                    "mixing_strategy": "sum_clip",
                    "encoder": {
                        "type": "aac",
                        "sample_rate": 44100
                    },
                    "channels": "stereo",
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtmp(
                smelter_core::protocols::RtmpOutputOptions {
                    connection: smelter_core::protocols::RtmpConnectionOptions {
                        host: "localhost".into(),
                        port: 1935,
                        app: "live".into(),
                        stream_key: "stream".into(),
                        use_tls: false,
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Ultrafast,
                            bitrate: Some(smelter_core::codecs::VideoEncoderBitrate {
                                average_bitrate: 4000000,
                                max_bitrate: 5000000,
                            }),
                            keyframe_interval: Duration::from_millis(2000),
                            resolution: smelter_render::Resolution {
                                width: 1280,
                                height: 720,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::Avcc,
                        },
                    )),
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::FdkAac(
                        smelter_core::codecs::FdkAacEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            sample_rate: 44100,
                            bitstream_format: smelter_core::codecs::AacBitstreamFormat::Raw,
                        },
                    )),
                },
            ),
            video: Some(default_video()),
            audio: Some(smelter_core::RegisterOutputAudioOptions {
                initial: smelter_core::AudioMixerConfig { inputs: vec![] },
                mixing_strategy: smelter_core::AudioMixingStrategy::SumClip,
                channels: smelter_core::AudioChannels::Stereo,
                end_condition: smelter_core::PipelineOutputEndCondition::Never,
            }),
        },
    );
}

#[test]
fn rtmp_vulkan_h264_encoder() {
    check_rtmp(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "vulkan_h264",
                        "keyframe_interval_ms": 3000
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtmp(
                smelter_core::protocols::RtmpOutputOptions {
                    connection: smelter_core::protocols::RtmpConnectionOptions {
                        host: "localhost".into(),
                        port: 1935,
                        app: "live".into(),
                        stream_key: "stream".into(),
                        use_tls: false,
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::VulkanH264(
                        smelter_core::codecs::VulkanH264EncoderOptions {
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            bitrate: None,
                            keyframe_interval: Duration::from_millis(3000),
                            preset: smelter_core::codecs::VulkanH264EncoderPreset::HighQuality,
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::Avcc,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn rtmp_vbr_bitrate() {
    check_rtmp(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264",
                        "bitrate": {
                            "average_bitrate": 4000000,
                            "max_bitrate": 6000000
                        }
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtmp(
                smelter_core::protocols::RtmpOutputOptions {
                    connection: smelter_core::protocols::RtmpConnectionOptions {
                        host: "localhost".into(),
                        port: 1935,
                        app: "live".into(),
                        stream_key: "stream".into(),
                        use_tls: false,
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: Some(smelter_core::codecs::VideoEncoderBitrate {
                                average_bitrate: 4000000,
                                max_bitrate: 6000000,
                            }),
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::Avcc,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn rtmp_send_eos_when_any_of() {
    check_rtmp(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "send_eos_when": {
                        "any_of": ["input_1", "input_2"]
                    },
                    "encoder": { "type": "ffmpeg_h264" },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtmp(
                smelter_core::protocols::RtmpOutputOptions {
                    connection: smelter_core::protocols::RtmpConnectionOptions {
                        host: "localhost".into(),
                        port: 1935,
                        app: "live".into(),
                        stream_key: "stream".into(),
                        use_tls: false,
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::Avcc,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(smelter_core::RegisterOutputVideoOptions {
                initial: smelter_render::scene::Component::View(
                    smelter_render::scene::ViewComponent::default(),
                ),
                end_condition: smelter_core::PipelineOutputEndCondition::AnyOf(vec![
                    smelter_render::InputId(Arc::from("input_1")),
                    smelter_render::InputId(Arc::from("input_2")),
                ]),
            }),
            audio: None,
        },
    );
}

#[test]
fn rtmp_send_eos_when_all_inputs() {
    check_rtmp(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "send_eos_when": {
                        "all_inputs": true
                    },
                    "encoder": { "type": "ffmpeg_h264" },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtmp(
                smelter_core::protocols::RtmpOutputOptions {
                    connection: smelter_core::protocols::RtmpConnectionOptions {
                        host: "localhost".into(),
                        port: 1935,
                        app: "live".into(),
                        stream_key: "stream".into(),
                        use_tls: false,
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::Avcc,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(smelter_core::RegisterOutputVideoOptions {
                initial: smelter_render::scene::Component::View(
                    smelter_render::scene::ViewComponent::default(),
                ),
                end_condition: smelter_core::PipelineOutputEndCondition::AllInputs,
            }),
            audio: None,
        },
    );
}

#[test]
fn err_rtmp_no_video_no_audio() {
    check_rtmp_err(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream"
            }
        }),
        "At least one of \"video\" and \"audio\" fields have to be specified.",
    );
}

#[test]
fn err_rtmp_vbr_max_less_than_average() {
    check_rtmp_err(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264",
                        "bitrate": {
                            "average_bitrate": 6000000,
                            "max_bitrate": 4000000
                        }
                    },
                    "initial": video_scene()
                }
            }
        }),
        "max_bitrate has to be greater than average_bitrate",
    );
}

#[test]
fn err_rtmp_negative_keyframe_interval() {
    check_rtmp_err(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264",
                        "keyframe_interval_ms": -1000
                    },
                    "initial": video_scene()
                }
            }
        }),
        "Keyframe interval cannot be negative.",
    );
}

#[test]
fn err_rtmp_conflicting_end_conditions() {
    check_rtmp_err(
        json!({
            "output": {
                "url": "rtmp://localhost:1935/live/stream",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "send_eos_when": {
                        "any_of": ["input_1"],
                        "all_inputs": true
                    },
                    "encoder": { "type": "ffmpeg_h264" },
                    "initial": video_scene()
                }
            }
        }),
        "Only one of \"any_of, all_of, any_input or all_inputs\" is allowed.",
    );
}

// ── RTP Output ───────────────────────────────────────────────────────

#[test]
fn rtp_udp_video_only() {
    check_rtp(
        json!({
            "output": {
                "port": 9002,
                "ip": "127.0.0.1",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264"
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtp(
                smelter_core::protocols::RtpOutputOptions {
                    connection_options: smelter_core::protocols::RtpOutputConnectionOptions::Udp {
                        port: smelter_core::protocols::Port(9002),
                        ip: Arc::from("127.0.0.1"),
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn rtp_tcp_server_video() {
    check_rtp(
        json!({
            "output": {
                "port": 9002,
                "transport_protocol": "tcp_server",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264"
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtp(
                smelter_core::protocols::RtpOutputOptions {
                    connection_options:
                        smelter_core::protocols::RtpOutputConnectionOptions::TcpServer {
                            port: smelter_core::protocols::PortOrRange::Exact(9002),
                        },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn rtp_tcp_server_port_range() {
    check_rtp(
        json!({
            "output": {
                "port": "9000:9010",
                "transport_protocol": "tcp_server",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": { "type": "ffmpeg_h264" },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtp(
                smelter_core::protocols::RtpOutputOptions {
                    connection_options:
                        smelter_core::protocols::RtpOutputConnectionOptions::TcpServer {
                            port: smelter_core::protocols::PortOrRange::Range((9000, 9010)),
                        },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn rtp_vp8_encoder() {
    check_rtp(
        json!({
            "output": {
                "port": 9002,
                "ip": "127.0.0.1",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_vp8",
                        "bitrate": 5000000,
                        "keyframe_interval_ms": 3000
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtp(
                smelter_core::protocols::RtpOutputOptions {
                    connection_options: smelter_core::protocols::RtpOutputConnectionOptions::Udp {
                        port: smelter_core::protocols::Port(9002),
                        ip: Arc::from("127.0.0.1"),
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegVp8(
                        smelter_core::codecs::FfmpegVp8EncoderOptions {
                            bitrate: Some(smelter_core::codecs::VideoEncoderBitrate {
                                average_bitrate: 5000000,
                                max_bitrate: 6250000,
                            }),
                            keyframe_interval: Duration::from_millis(3000),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            raw_options: vec![],
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn rtp_vp9_encoder() {
    check_rtp(
        json!({
            "output": {
                "port": 9002,
                "ip": "127.0.0.1",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_vp9",
                        "pixel_format": "yuv444p"
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtp(
                smelter_core::protocols::RtpOutputOptions {
                    connection_options: smelter_core::protocols::RtpOutputConnectionOptions::Udp {
                        port: smelter_core::protocols::Port(9002),
                        ip: Arc::from("127.0.0.1"),
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegVp9(
                        smelter_core::codecs::FfmpegVp9EncoderOptions {
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV444P,
                            raw_options: vec![],
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn rtp_audio_opus() {
    check_rtp(
        json!({
            "output": {
                "port": 9002,
                "ip": "127.0.0.1",
                "audio": {
                    "encoder": {
                        "type": "opus",
                        "preset": "voip",
                        "sample_rate": 48000,
                        "forward_error_correction": true,
                        "expected_packet_loss": 10
                    },
                    "channels": "mono",
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtp(
                smelter_core::protocols::RtpOutputOptions {
                    connection_options: smelter_core::protocols::RtpOutputConnectionOptions::Udp {
                        port: smelter_core::protocols::Port(9002),
                        ip: Arc::from("127.0.0.1"),
                    },
                    video: None,
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::Opus(
                        smelter_core::codecs::OpusEncoderOptions {
                            channels: smelter_core::AudioChannels::Mono,
                            preset: smelter_core::codecs::OpusEncoderPreset::Voip,
                            sample_rate: 48000,
                            forward_error_correction: true,
                            packet_loss: 10,
                        },
                    )),
                },
            ),
            video: None,
            audio: Some(smelter_core::RegisterOutputAudioOptions {
                initial: smelter_core::AudioMixerConfig { inputs: vec![] },
                mixing_strategy: smelter_core::AudioMixingStrategy::SumClip,
                channels: smelter_core::AudioChannels::Mono,
                end_condition: smelter_core::PipelineOutputEndCondition::Never,
            }),
        },
    );
}

#[test]
fn rtp_video_and_audio() {
    check_rtp(
        json!({
            "output": {
                "port": 9002,
                "ip": "127.0.0.1",
                "video": {
                    "resolution": { "width": 1280, "height": 720 },
                    "encoder": { "type": "ffmpeg_h264", "preset": "medium" },
                    "initial": video_scene()
                },
                "audio": {
                    "encoder": { "type": "opus" },
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Rtp(
                smelter_core::protocols::RtpOutputOptions {
                    connection_options: smelter_core::protocols::RtpOutputConnectionOptions::Udp {
                        port: smelter_core::protocols::Port(9002),
                        ip: Arc::from("127.0.0.1"),
                    },
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Medium,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1280,
                                height: 720,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::Opus(
                        smelter_core::codecs::OpusEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            preset: smelter_core::codecs::OpusEncoderPreset::Voip,
                            sample_rate: 48000,
                            forward_error_correction: false,
                            packet_loss: 0,
                        },
                    )),
                },
            ),
            video: Some(default_video()),
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn err_rtp_no_video_no_audio() {
    check_rtp_err(
        json!({
            "output": {
                "port": 9002,
                "ip": "127.0.0.1"
            }
        }),
        "At least one of \"video\" and \"audio\" fields have to be specified.",
    );
}

#[test]
fn err_rtp_udp_port_range() {
    check_rtp_err(
        json!({
            "output": {
                "port": "9000:9010",
                "ip": "127.0.0.1",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": { "type": "ffmpeg_h264" },
                    "initial": video_scene()
                }
            }
        }),
        "Port range can not be used with UDP output stream (transport_protocol=\"udp\").",
    );
}

#[test]
fn err_rtp_udp_missing_ip() {
    check_rtp_err(
        json!({
            "output": {
                "port": 9002,
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": { "type": "ffmpeg_h264" },
                    "initial": video_scene()
                }
            }
        }),
        "\"ip\" field is required when registering output UDP stream (transport_protocol=\"udp\").",
    );
}

#[test]
fn err_rtp_tcp_server_with_ip() {
    check_rtp_err(
        json!({
            "output": {
                "port": 9002,
                "ip": "127.0.0.1",
                "transport_protocol": "tcp_server",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": { "type": "ffmpeg_h264" },
                    "initial": video_scene()
                }
            }
        }),
        "\"ip\" field is not allowed when registering TCP server connection (transport_protocol=\"tcp_server\").",
    );
}

#[test]
fn err_rtp_opus_packet_loss_out_of_range() {
    check_rtp_err(
        json!({
            "output": {
                "port": 9002,
                "ip": "127.0.0.1",
                "audio": {
                    "encoder": {
                        "type": "opus",
                        "expected_packet_loss": 101
                    },
                    "initial": audio_scene()
                }
            }
        }),
        "Expected packet loss value must be from [0, 100] range.",
    );
}

// ── MP4 Output ───────────────────────────────────────────────────────

#[test]
fn mp4_video_only() {
    check_mp4(
        json!({
            "output": {
                "path": "/tmp/output.mp4",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264",
                        "preset": "slow"
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Mp4(
                smelter_core::protocols::Mp4OutputOptions {
                    output_path: Arc::from(Path::new("/tmp/output.mp4")),
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Slow,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: None,
                    raw_options: vec![],
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn mp4_audio_only() {
    check_mp4(
        json!({
            "output": {
                "path": "/tmp/output.mp4",
                "audio": {
                    "encoder": { "type": "aac", "sample_rate": 48000 },
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Mp4(
                smelter_core::protocols::Mp4OutputOptions {
                    output_path: Arc::from(Path::new("/tmp/output.mp4")),
                    video: None,
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::FdkAac(
                        smelter_core::codecs::FdkAacEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            sample_rate: 48000,
                            bitstream_format: smelter_core::codecs::AacBitstreamFormat::Raw,
                        },
                    )),
                    raw_options: vec![],
                },
            ),
            video: None,
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn mp4_video_and_audio_with_ffmpeg_options() {
    check_mp4(
        json!({
            "output": {
                "path": "/tmp/output.mp4",
                "video": {
                    "resolution": { "width": 1280, "height": 720 },
                    "encoder": {
                        "type": "ffmpeg_h264",
                        "ffmpeg_options": { "crf": "23" }
                    },
                    "initial": video_scene()
                },
                "audio": {
                    "mixing_strategy": "sum_scale",
                    "encoder": { "type": "aac" },
                    "channels": "mono",
                    "initial": audio_scene()
                },
                "ffmpeg_options": { "movflags": "faststart" }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Mp4(
                smelter_core::protocols::Mp4OutputOptions {
                    output_path: Arc::from(Path::new("/tmp/output.mp4")),
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1280,
                                height: 720,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![(Arc::from("crf"), Arc::from("23"))],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::FdkAac(
                        smelter_core::codecs::FdkAacEncoderOptions {
                            channels: smelter_core::AudioChannels::Mono,
                            sample_rate: 44100,
                            bitstream_format: smelter_core::codecs::AacBitstreamFormat::Raw,
                        },
                    )),
                    raw_options: vec![(Arc::from("movflags"), Arc::from("faststart"))],
                },
            ),
            video: Some(default_video()),
            audio: Some(smelter_core::RegisterOutputAudioOptions {
                initial: smelter_core::AudioMixerConfig { inputs: vec![] },
                mixing_strategy: smelter_core::AudioMixingStrategy::SumScale,
                channels: smelter_core::AudioChannels::Mono,
                end_condition: smelter_core::PipelineOutputEndCondition::Never,
            }),
        },
    );
}

#[test]
fn mp4_vulkan_encoder() {
    check_mp4(
        json!({
            "output": {
                "path": "/tmp/output.mp4",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": { "type": "vulkan_h264" },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Mp4(
                smelter_core::protocols::Mp4OutputOptions {
                    output_path: Arc::from(Path::new("/tmp/output.mp4")),
                    video: Some(smelter_core::codecs::VideoEncoderOptions::VulkanH264(
                        smelter_core::codecs::VulkanH264EncoderOptions {
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            preset: smelter_core::codecs::VulkanH264EncoderPreset::HighQuality,
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: None,
                    raw_options: vec![],
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn err_mp4_no_video_no_audio() {
    check_mp4_err(
        json!({
            "output": {
                "path": "/tmp/output.mp4"
            }
        }),
        "At least one of \"video\" and \"audio\" fields have to be specified.",
    );
}

// ── WHIP Output ──────────────────────────────────────────────────────

#[test]
fn whip_video_only() {
    check_whip(
        json!({
            "output": {
                "endpoint_url": "https://example.com/whip",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Whip(
                smelter_core::protocols::WhipOutputOptions {
                    endpoint_url: Arc::from("https://example.com/whip"),
                    bearer_token: None,
                    video: Some(smelter_core::protocols::VideoWhipOptions {
                        encoder_preferences: vec![
                            smelter_core::protocols::WhipVideoEncoderOptions::Any(
                                smelter_render::Resolution {
                                    width: 1920,
                                    height: 1080,
                                },
                            ),
                        ],
                    }),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn whip_video_with_encoder_preferences() {
    check_whip(
        json!({
            "output": {
                "endpoint_url": "https://example.com/whip",
                "bearer_token": "token",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder_preferences": [
                        { "type": "ffmpeg_h264", "preset": "fast" },
                        { "type": "ffmpeg_vp8" },
                        { "type": "any" }
                    ],
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Whip(
                smelter_core::protocols::WhipOutputOptions {
                    endpoint_url: Arc::from("https://example.com/whip"),
                    bearer_token: Some(Arc::from("token")),
                    video: Some(smelter_core::protocols::VideoWhipOptions {
                        encoder_preferences: vec![
                            smelter_core::protocols::WhipVideoEncoderOptions::FfmpegH264(
                                smelter_core::codecs::FfmpegH264EncoderOptions {
                                    preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                                    bitrate: None,
                                    keyframe_interval: default_keyframe_interval(),
                                    resolution: smelter_render::Resolution {
                                        width: 1920,
                                        height: 1080,
                                    },
                                    pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                                    raw_options: vec![],
                                    bitstream_format:
                                        smelter_core::codecs::H264BitstreamFormat::AnnexB,
                                },
                            ),
                            smelter_core::protocols::WhipVideoEncoderOptions::FfmpegVp8(
                                smelter_core::codecs::FfmpegVp8EncoderOptions {
                                    bitrate: None,
                                    keyframe_interval: default_keyframe_interval(),
                                    resolution: smelter_render::Resolution {
                                        width: 1920,
                                        height: 1080,
                                    },
                                    raw_options: vec![],
                                },
                            ),
                            smelter_core::protocols::WhipVideoEncoderOptions::Any(
                                smelter_render::Resolution {
                                    width: 1920,
                                    height: 1080,
                                },
                            ),
                        ],
                    }),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn whip_audio_only() {
    check_whip(
        json!({
            "output": {
                "endpoint_url": "https://example.com/whip",
                "audio": {
                    "encoder_preferences": [
                        { "type": "opus", "preset": "quality" }
                    ],
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Whip(
                smelter_core::protocols::WhipOutputOptions {
                    endpoint_url: Arc::from("https://example.com/whip"),
                    bearer_token: None,
                    video: None,
                    audio: Some(smelter_core::protocols::AudioWhipOptions {
                        encoder_preferences: vec![
                            smelter_core::protocols::WhipAudioEncoderOptions::Opus(
                                smelter_core::codecs::OpusEncoderOptions {
                                    channels: smelter_core::AudioChannels::Stereo,
                                    preset: smelter_core::codecs::OpusEncoderPreset::Quality,
                                    sample_rate: 48000,
                                    forward_error_correction: true,
                                    packet_loss: 0,
                                },
                            ),
                        ],
                    }),
                },
            ),
            video: None,
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn whip_video_and_audio() {
    check_whip(
        json!({
            "output": {
                "endpoint_url": "https://example.com/whip",
                "video": {
                    "resolution": { "width": 1280, "height": 720 },
                    "encoder_preferences": [
                        { "type": "ffmpeg_vp9", "pixel_format": "yuv420p" }
                    ],
                    "initial": video_scene()
                },
                "audio": {
                    "encoder_preferences": [{ "type": "opus" }, { "type": "any" }],
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Whip(
                smelter_core::protocols::WhipOutputOptions {
                    endpoint_url: Arc::from("https://example.com/whip"),
                    bearer_token: None,
                    video: Some(smelter_core::protocols::VideoWhipOptions {
                        encoder_preferences: vec![
                            smelter_core::protocols::WhipVideoEncoderOptions::FfmpegVp9(
                                smelter_core::codecs::FfmpegVp9EncoderOptions {
                                    resolution: smelter_render::Resolution {
                                        width: 1280,
                                        height: 720,
                                    },
                                    bitrate: None,
                                    keyframe_interval: default_keyframe_interval(),
                                    pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                                    raw_options: vec![],
                                },
                            ),
                        ],
                    }),
                    audio: Some(smelter_core::protocols::AudioWhipOptions {
                        encoder_preferences: vec![
                            smelter_core::protocols::WhipAudioEncoderOptions::Opus(
                                smelter_core::codecs::OpusEncoderOptions {
                                    channels: smelter_core::AudioChannels::Stereo,
                                    preset: smelter_core::codecs::OpusEncoderPreset::Voip,
                                    sample_rate: 48000,
                                    forward_error_correction: true,
                                    packet_loss: 0,
                                },
                            ),
                            smelter_core::protocols::WhipAudioEncoderOptions::Any(
                                smelter_core::AudioChannels::Stereo,
                            ),
                        ],
                    }),
                },
            ),
            video: Some(default_video()),
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn err_whip_no_video_no_audio() {
    check_whip_err(
        json!({
            "output": {
                "endpoint_url": "https://example.com/whip"
            }
        }),
        "At least one of \"video\" and \"audio\" fields have to be specified.",
    );
}

// ── WHEP Output ──────────────────────────────────────────────────────

#[test]
fn whep_video_only() {
    check_whep(
        json!({
            "output": {
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264"
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Whep(
                smelter_core::protocols::WhepOutputOptions {
                    bearer_token: None,
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn whep_audio_only() {
    check_whep(
        json!({
            "output": {
                "audio": {
                    "encoder": {
                        "type": "opus",
                        "forward_error_correction": true,
                        "expected_packet_loss": 50
                    },
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Whep(
                smelter_core::protocols::WhepOutputOptions {
                    bearer_token: None,
                    video: None,
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::Opus(
                        smelter_core::codecs::OpusEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            preset: smelter_core::codecs::OpusEncoderPreset::Voip,
                            sample_rate: 48000,
                            forward_error_correction: true,
                            packet_loss: 50,
                        },
                    )),
                },
            ),
            video: None,
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn whep_video_and_audio_with_bearer_token() {
    check_whep(
        json!({
            "output": {
                "bearer_token": "secret",
                "video": {
                    "resolution": { "width": 1280, "height": 720 },
                    "encoder": { "type": "ffmpeg_vp8" },
                    "initial": video_scene()
                },
                "audio": {
                    "encoder": { "type": "opus" },
                    "channels": "stereo",
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Whep(
                smelter_core::protocols::WhepOutputOptions {
                    bearer_token: Some(Arc::from("secret")),
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegVp8(
                        smelter_core::codecs::FfmpegVp8EncoderOptions {
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1280,
                                height: 720,
                            },
                            raw_options: vec![],
                        },
                    )),
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::Opus(
                        smelter_core::codecs::OpusEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            preset: smelter_core::codecs::OpusEncoderPreset::Voip,
                            sample_rate: 48000,
                            forward_error_correction: true,
                            packet_loss: 0,
                        },
                    )),
                },
            ),
            video: Some(default_video()),
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn whep_vp9_encoder() {
    check_whep(
        json!({
            "output": {
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_vp9",
                        "pixel_format": "yuv422p",
                        "bitrate": 5000000
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Whep(
                smelter_core::protocols::WhepOutputOptions {
                    bearer_token: None,
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegVp9(
                        smelter_core::codecs::FfmpegVp9EncoderOptions {
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            bitrate: Some(smelter_core::codecs::VideoEncoderBitrate {
                                average_bitrate: 5000000,
                                max_bitrate: 6250000,
                            }),
                            keyframe_interval: default_keyframe_interval(),
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV422P,
                            raw_options: vec![],
                        },
                    )),
                    audio: None,
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn err_whep_no_video_no_audio() {
    check_whep_err(
        json!({
            "output": {}
        }),
        "At least one of \"video\" and \"audio\" fields have to be specified.",
    );
}

#[test]
fn err_whep_opus_packet_loss_out_of_range() {
    check_whep_err(
        json!({
            "output": {
                "audio": {
                    "encoder": {
                        "type": "opus",
                        "expected_packet_loss": 200
                    },
                    "initial": audio_scene()
                }
            }
        }),
        "Expected packet loss value must be from [0, 100] range.",
    );
}

// ── HLS Output ───────────────────────────────────────────────────────

#[test]
fn hls_video_only() {
    check_hls(
        json!({
            "output": {
                "path": "/tmp/stream.m3u8",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": {
                        "type": "ffmpeg_h264",
                        "preset": "veryfast"
                    },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Hls(
                smelter_core::protocols::HlsOutputOptions {
                    output_path: Arc::from(Path::new("/tmp/stream.m3u8")),
                    max_playlist_size: None,
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Veryfast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: None,
                    raw_options: vec![],
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn hls_audio_only() {
    check_hls(
        json!({
            "output": {
                "path": "/tmp/stream.m3u8",
                "audio": {
                    "encoder": { "type": "aac", "sample_rate": 48000 },
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Hls(
                smelter_core::protocols::HlsOutputOptions {
                    output_path: Arc::from(Path::new("/tmp/stream.m3u8")),
                    max_playlist_size: None,
                    video: None,
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::FdkAac(
                        smelter_core::codecs::FdkAacEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            sample_rate: 48000,
                            bitstream_format: smelter_core::codecs::AacBitstreamFormat::Raw,
                        },
                    )),
                    raw_options: vec![],
                },
            ),
            video: None,
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn hls_video_and_audio_with_playlist_size() {
    check_hls(
        json!({
            "output": {
                "path": "/tmp/stream.m3u8",
                "max_playlist_size": 10,
                "video": {
                    "resolution": { "width": 1280, "height": 720 },
                    "encoder": { "type": "ffmpeg_h264" },
                    "initial": video_scene()
                },
                "audio": {
                    "encoder": { "type": "aac" },
                    "initial": audio_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Hls(
                smelter_core::protocols::HlsOutputOptions {
                    output_path: Arc::from(Path::new("/tmp/stream.m3u8")),
                    max_playlist_size: Some(10),
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1280,
                                height: 720,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::FdkAac(
                        smelter_core::codecs::FdkAacEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            sample_rate: 44100,
                            bitstream_format: smelter_core::codecs::AacBitstreamFormat::Raw,
                        },
                    )),
                    raw_options: vec![],
                },
            ),
            video: Some(default_video()),
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn hls_vulkan_encoder() {
    check_hls(
        json!({
            "output": {
                "path": "/tmp/stream.m3u8",
                "video": {
                    "resolution": { "width": 1920, "height": 1080 },
                    "encoder": { "type": "vulkan_h264" },
                    "initial": video_scene()
                }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Hls(
                smelter_core::protocols::HlsOutputOptions {
                    output_path: Arc::from(Path::new("/tmp/stream.m3u8")),
                    max_playlist_size: None,
                    video: Some(smelter_core::codecs::VideoEncoderOptions::VulkanH264(
                        smelter_core::codecs::VulkanH264EncoderOptions {
                            resolution: smelter_render::Resolution {
                                width: 1920,
                                height: 1080,
                            },
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            preset: smelter_core::codecs::VulkanH264EncoderPreset::HighQuality,
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: None,
                    raw_options: vec![],
                },
            ),
            video: Some(default_video()),
            audio: None,
        },
    );
}

#[test]
fn hls_video_and_audio_with_ffmpeg_options() {
    check_hls(
        json!({
            "output": {
                "path": "/tmp/stream.m3u8",
                "video": {
                    "resolution": { "width": 1280, "height": 720 },
                    "encoder": {
                        "type": "ffmpeg_h264",
                        "ffmpeg_options": { "crf": "23" }
                    },
                    "initial": video_scene()
                },
                "audio": {
                    "encoder": { "type": "aac" },
                    "initial": audio_scene()
                },
                "ffmpeg_options": { "hls_list_size": "5" }
            }
        }),
        CoreOutput {
            output_options: smelter_core::ProtocolOutputOptions::Hls(
                smelter_core::protocols::HlsOutputOptions {
                    output_path: Arc::from(Path::new("/tmp/stream.m3u8")),
                    max_playlist_size: None,
                    video: Some(smelter_core::codecs::VideoEncoderOptions::FfmpegH264(
                        smelter_core::codecs::FfmpegH264EncoderOptions {
                            preset: smelter_core::codecs::FfmpegH264EncoderPreset::Fast,
                            bitrate: None,
                            keyframe_interval: default_keyframe_interval(),
                            resolution: smelter_render::Resolution {
                                width: 1280,
                                height: 720,
                            },
                            pixel_format: smelter_core::codecs::OutputPixelFormat::YUV420P,
                            raw_options: vec![(Arc::from("crf"), Arc::from("23"))],
                            bitstream_format: smelter_core::codecs::H264BitstreamFormat::AnnexB,
                        },
                    )),
                    audio: Some(smelter_core::codecs::AudioEncoderOptions::FdkAac(
                        smelter_core::codecs::FdkAacEncoderOptions {
                            channels: smelter_core::AudioChannels::Stereo,
                            sample_rate: 44100,
                            bitstream_format: smelter_core::codecs::AacBitstreamFormat::Raw,
                        },
                    )),
                    raw_options: vec![(Arc::from("hls_list_size"), Arc::from("5"))],
                },
            ),
            video: Some(default_video()),
            audio: Some(default_audio()),
        },
    );
}

#[test]
fn err_hls_no_video_no_audio() {
    check_hls_err(
        json!({
            "output": {
                "path": "/tmp/stream.m3u8"
            }
        }),
        "At least one of \"video\" and \"audio\" fields have to be specified.",
    );
}

// ── Serde-level errors ──────────────────────────────────────────────

#[test]
fn err_serde_rtmp_missing_url() {
    check_serde_err::<RtmpOutput>(json!({
        "output": {
            "video": {
                "resolution": { "width": 1920, "height": 1080 },
                "encoder": { "type": "ffmpeg_h264" },
                "initial": video_scene()
            }
        }
    }));
}

#[test]
fn err_serde_rtp_missing_port() {
    check_serde_err::<RtpOutput>(json!({
        "output": {
            "ip": "127.0.0.1",
            "video": {
                "resolution": { "width": 1920, "height": 1080 },
                "encoder": { "type": "ffmpeg_h264" },
                "initial": video_scene()
            }
        }
    }));
}

#[test]
fn err_serde_mp4_missing_path() {
    check_serde_err::<Mp4Output>(json!({
        "output": {
            "video": {
                "resolution": { "width": 1920, "height": 1080 },
                "encoder": { "type": "ffmpeg_h264" },
                "initial": video_scene()
            }
        }
    }));
}

#[test]
fn err_serde_rtmp_unknown_field() {
    check_serde_err::<RtmpOutput>(json!({
        "output": {
            "url": "rtmp://localhost:1935/live/stream",
            "unknown_field": true
        }
    }));
}

#[test]
fn err_serde_rtp_unknown_encoder() {
    check_serde_err::<RtpOutput>(json!({
        "output": {
            "port": 9002,
            "ip": "127.0.0.1",
            "video": {
                "resolution": { "width": 1920, "height": 1080 },
                "encoder": { "type": "unknown_encoder" },
                "initial": video_scene()
            }
        }
    }));
}

#[test]
fn err_serde_whip_missing_endpoint() {
    check_serde_err::<WhipOutput>(json!({
        "output": {
            "video": {
                "resolution": { "width": 1920, "height": 1080 },
                "initial": video_scene()
            }
        }
    }));
}

#[test]
fn err_serde_hls_missing_path() {
    check_serde_err::<HlsOutput>(json!({
        "output": {
            "video": {
                "resolution": { "width": 1920, "height": 1080 },
                "encoder": { "type": "ffmpeg_h264" },
                "initial": video_scene()
            }
        }
    }));
}

fn moq_aac_request(container: Option<&str>) -> serde_json::Value {
    let mut output = json!({
        "endpoint_url": "https://localhost:443",
        "broadcast_path": "anon/test",
        "audio": {
            "encoder": { "type": "aac" },
            "initial": audio_scene()
        }
    });
    if let Some(container) = container {
        output
            .as_object_mut()
            .unwrap()
            .insert("container".to_string(), json!(container));
    }
    json!({ "output": output })
}

fn moq_aac_expected(
    container: smelter_core::protocols::MoqOutputContainer,
    bitstream_format: smelter_core::codecs::AacBitstreamFormat,
) -> CoreOutput {
    CoreOutput {
        output_options: smelter_core::ProtocolOutputOptions::MoqClient(
            smelter_core::protocols::MoqClientOutputOptions {
                endpoint_url: "https://localhost:443".into(),
                broadcast_path: "anon/test".into(),
                container,
                video: None,
                audio: Some(smelter_core::codecs::AudioEncoderOptions::FdkAac(
                    smelter_core::codecs::FdkAacEncoderOptions {
                        channels: smelter_core::AudioChannels::Stereo,
                        sample_rate: 44100,
                        bitstream_format,
                    },
                )),
            },
        ),
        video: None,
        audio: Some(default_audio()),
    }
}

#[test]
fn moq_client_aac_cmaf() {
    check_moq(
        moq_aac_request(Some("cmaf")),
        moq_aac_expected(
            smelter_core::protocols::MoqOutputContainer::Cmaf,
            smelter_core::codecs::AacBitstreamFormat::Raw,
        ),
    );
}

#[test]
fn moq_client_aac_legacy() {
    check_moq(
        moq_aac_request(Some("legacy")),
        moq_aac_expected(
            smelter_core::protocols::MoqOutputContainer::Legacy,
            smelter_core::codecs::AacBitstreamFormat::Adts,
        ),
    );
}

#[test]
fn moq_client_aac_loc() {
    check_moq(
        moq_aac_request(Some("loc")),
        moq_aac_expected(
            smelter_core::protocols::MoqOutputContainer::Loc,
            smelter_core::codecs::AacBitstreamFormat::Adts,
        ),
    );
}

#[test]
fn moq_client_aac_default_container() {
    // No container specified defaults to CMAF, which requires raw access units.
    check_moq(
        moq_aac_request(None),
        moq_aac_expected(
            smelter_core::protocols::MoqOutputContainer::Cmaf,
            smelter_core::codecs::AacBitstreamFormat::Raw,
        ),
    );
}
