use anyhow::Result;
use serde_json::json;
use std::time::Duration;

use crate::{
    compare_audio_dumps, input_dump_from_disk, AudioValidationConfig, CommunicationProtocol,
    CompositorInstance, OutputReceiver, PacketSender,
};

/// Two audio input streams mixed together with different volumes.
///
/// Play mixed audio for 20 seconds.
#[test]
pub fn audio_mixing_with_offset() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "audio_mixing_with_offset_output.rtp";
    let instance = CompositorInstance::start(None);
    let input_1_port = instance.get_port();
    let input_2_port = instance.get_port();
    let output_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "audio": {
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_1",
                            "volume": 0.3,
                        },
                        {
                            "input_id": "input_2",
                            "volume": 0.7,
                        }
                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                }
            },
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

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
            "port": input_1_port,
            "audio": {
                "decoder": "opus"
            },
            "offset_ms": 0,
        }),
    )?;

    instance.send_request(
        "input/input_2/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_2_port,
            "audio": {
                "decoder": "opus"
            },
            "offset_ms": 0,
        }),
    )?;

    let audio_input_1 = input_dump_from_disk("countdown_audio.rtp")?;
    let audio_input_2 = input_dump_from_disk("8_colors_input_reversed_audio.rtp")?;
    PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?.send(&audio_input_1)?;
    PacketSender::new(CommunicationProtocol::Tcp, input_2_port)?.send(&audio_input_2)?;

    instance.send_request("start", json!({}))?;

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        AudioValidationConfig {
            sampling_intervals: vec![Duration::from_millis(0)..Duration::from_millis(10000)],
            ..Default::default()
        },
    )?;

    Ok(())
}

/// Two audio input streams mixed together with different volumes.
/// No offset on inputs so it relies on race condition and might be flaky.
///
/// Play mixed audio for 20 seconds.
#[test]
pub fn audio_mixing_no_offset() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "audio_mixing_no_offset_output.rtp";
    let instance = CompositorInstance::start(None);
    let input_1_port = instance.get_port();
    let input_2_port = instance.get_port();
    let output_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "audio": {
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_1",
                            "volume": 0.3,
                        },
                        {
                            "input_id": "input_2",
                            "volume": 0.7,
                        }
                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                }
            },
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

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
            "port": input_1_port,
            "audio": {
                "decoder": "opus"
            },
        }),
    )?;

    instance.send_request(
        "input/input_2/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_2_port,
            "audio": {
                "decoder": "opus"
            },
        }),
    )?;

    let audio_input_1 = input_dump_from_disk("countdown_audio.rtp")?;
    let audio_input_2 = input_dump_from_disk("8_colors_input_reversed_audio.rtp")?;
    let audio_1_sender = PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?;
    let audio_2_sender = PacketSender::new(CommunicationProtocol::Tcp, input_2_port)?;

    let audio_1_handle = audio_1_sender.send_non_blocking(audio_input_1);
    let audio_2_handle = audio_2_sender.send_non_blocking(audio_input_2);

    instance.send_request("start", json!({}))?;

    audio_1_handle.join().unwrap();
    audio_2_handle.join().unwrap();
    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        AudioValidationConfig {
            sampling_intervals: vec![Duration::from_millis(0)..Duration::from_millis(10000)],
            ..Default::default()
        },
    )?;

    Ok(())
}

/// Single input with audio of a countdown.
///
/// Play audio for 20 seconds, the last few second should be silent
#[test]
pub fn single_input_opus() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "single_input_opus_output.rtp";
    let instance = CompositorInstance::start(None);
    let input_1_port = instance.get_port();
    let output_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "audio": {
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_1",
                        },

                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                }
            },
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

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
            "port": input_1_port,
            "audio": {
                "decoder": "opus"
            },
            "offset_ms": 0,
        }),
    )?;

    let audio_input_1 = input_dump_from_disk("a_libopus_audio.rtp")?;
    PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?.send(&audio_input_1)?;

    instance.send_request("start", json!({}))?;

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        AudioValidationConfig {
            sampling_intervals: vec![Duration::from_millis(0)..Duration::from_millis(10000)],
            ..Default::default()
        },
    )?;

    Ok(())
}

/// An AAC audio input stream.
///
/// Play audio for 10 seconds.
#[test]
pub fn single_input_aac() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "single_input_aac_output.rtp";
    let instance = CompositorInstance::start(None);
    let input_1_port = instance.get_port();
    let output_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "audio": {
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_1",
                            "volume": 1.0,
                        },
                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                }
            },
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 10000,
        }),
    )?;

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_1_port,
            "audio": {
                "decoder": "aac",
                "audio_specific_config": "1210",
                "rtp_mode": "high_bitrate",
            },
            "offset_ms": 0,
        }),
    )?;

    let audio_input_1 = input_dump_from_disk("big_buck_bunny_10s_audio_aac.rtp")?;
    PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?.send(&audio_input_1)?;

    instance.send_request("start", json!({}))?;

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        AudioValidationConfig {
            sampling_intervals: vec![Duration::from_millis(0)..Duration::from_millis(10000)],
            ..Default::default()
        },
    )?;

    Ok(())
}
