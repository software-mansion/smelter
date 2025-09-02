use anyhow::Result;
use serde_json::json;
use std::time::Duration;

use crate::{
    audio::{self, AudioAnalyzeTolerance, AudioValidationConfig, RealFrequencyTolerance},
    compare_audio_dumps, compare_video_dumps, input_dump_from_disk,
    video::VideoValidationConfig,
    CommunicationProtocol, CompositorInstance, OutputReceiver, PacketSender,
};

/// Input and output streams with muxed video and audio.
///
/// Show `input_1` with audio for 20 seconds.
#[test]
pub fn single_input_with_video_and_audio_flaky() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "single_input_with_video_and_audio_output.rtp";
    let instance = CompositorInstance::start(None);
    let input_port = instance.get_port();
    let output_port = instance.get_port();

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Udp)?;

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "ip": "127.0.0.1",
            "port": output_port,
            "video": {
                "resolution": {
                    "width": 640,
                    "height": 360,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                },
                "initial": {
                    "root": {
                        "id": "input_1",
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            },
            "audio": {
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_1",
                        }
                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                }
            }
        }),
    )?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20000,
        }),
    )?;

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "audio": {
                "decoder": "opus"
            }
        }),
    )?;

    let packets_dump = input_dump_from_disk("8_colors_input_video_audio.rtp")?;
    let sender_handle =
        PacketSender::new(CommunicationProtocol::Tcp, input_port)?.send_non_blocking(packets_dump);

    instance.send_request("start", json!({}))?;

    sender_handle.join().unwrap();
    let new_output_dump = output_receiver.wait_for_output()?;

    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        VideoValidationConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_secs(18)],
            ..Default::default()
        },
    )?;

    let audio_validation_tolerance = RealFrequencyTolerance {
        max_frequency_level: 5.0,
        average_level: 15.0,
        median_level: 15.0,
        general_level: 5.0,
        ..Default::default()
    };

    let audio_validation_config = AudioValidationConfig {
        tolerance: AudioAnalyzeTolerance {
            frequency_tolerance: audio::FrequencyTolerance::Real(audio_validation_tolerance),
            ..Default::default()
        },
        ..Default::default()
    };

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        audio::ValidationMode::Real,
        audio_validation_config,
    )?;

    Ok(())
}
