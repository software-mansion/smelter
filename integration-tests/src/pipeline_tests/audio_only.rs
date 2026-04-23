use std::{thread, time::Duration};

use anyhow::Result;
use integration_tests_macros::pipeline_test;
use serde_json::json;

use crate::{
    CommunicationProtocol, CompositorInstance, OutputReceiver, PacketSender,
    audio::{self, AudioAnalyzeTolerance, AudioValidationConfig},
    compare_audio_dumps, input_dump_from_disk,
    paths::submodule_root_path,
    pipeline_tests::PipelineTest,
};

#[allow(dead_code)]
pub const TESTS: &[PipelineTest] = &[
    AUDIO_MIXING_WITH_OFFSET,
    AUDIO_MIXING_NO_OFFSET,
    AUDIO_MIXING_TRACK_INSERTION_WITH_OFFSET,
    SINGLE_INPUT_OPUS,
    SINGLE_INPUT_AAC,
    SINGLE_INPUT_AAC_MP4,
    AUDIO_EARLY_STREAMING_WITH_OFFSET,
    AUDIO_EARLY_STREAMING_NO_OFFSET,
];

#[pipeline_test(
    description = "
        Two audio input streams mixed together with different volumes.

        Play mixed audio for 20 seconds.
    ",
    snapshot_name = "audio_mixing_with_offset_output.rtp"
)]
pub fn audio_mixing_with_offset() -> Result<()> {
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

    let audio_input_1 = input_dump_from_disk("a_opus_audio.rtp")?;
    let audio_input_2 = input_dump_from_disk("c_sharp_opus_audio.rtp")?;
    PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?.send(&audio_input_1)?;
    PacketSender::new(CommunicationProtocol::Tcp, input_2_port)?.send(&audio_input_2)?;

    instance.send_request("start", json!({}))?;

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        audio::ValidationMode::Artificial,
        AudioValidationConfig::default(),
    )?;

    Ok(())
}

#[pipeline_test(
    description = "
        Two audio input streams mixed together with different volumes.
        No offset on inputs so it relies on race condition and might be flaky.

        Play mixed audio for 20 seconds.
    ",
    snapshot_name = "audio_mixing_no_offset_output.rtp"
)]
pub fn audio_mixing_no_offset() -> Result<()> {
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

    let audio_input_1 = input_dump_from_disk("a_opus_audio.rtp")?;
    let audio_input_2 = input_dump_from_disk("c_sharp_opus_audio.rtp")?;
    let audio_1_sender = PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?;
    let audio_2_sender = PacketSender::new(CommunicationProtocol::Tcp, input_2_port)?;

    let audio_1_handle = audio_1_sender.send_non_blocking(audio_input_1);
    let audio_2_handle = audio_2_sender.send_non_blocking(audio_input_2);

    instance.send_request("start", json!({}))?;

    audio_1_handle.join().unwrap();
    audio_2_handle.join().unwrap();
    let new_output_dump = output_receiver.wait_for_output()?;

    // This test is flaky due to no_offset being set so we allow 1 failed batch per channel
    // (usually fails first batch)
    let audio_validation_config = AudioValidationConfig {
        tolerance: AudioAnalyzeTolerance {
            allowed_failed_batches: 2,
            ..Default::default()
        },
        ..Default::default()
    };

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        audio::ValidationMode::Artificial,
        audio_validation_config,
    )?;

    Ok(())
}

#[pipeline_test(
    description = "
        Two audio input streams mixed together with the same volume after update request.
        Second one joins after 10 seconds of `thread::sleep`.

        Play audio for 20 seconds.
    ",
    snapshot_name = "audio_mixing_track_insertion_with_offset_output.rtp"
)]
pub fn audio_mixing_track_insertion_with_offset() -> Result<()> {
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
                            "volume": 0.5,
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

    let audio_input_1 = input_dump_from_disk("a_opus_audio.rtp")?;
    let audio_input_2 = input_dump_from_disk("c_sharp_opus_audio.rtp")?;
    let audio_1_sender = PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?;
    let audio_2_sender = PacketSender::new(CommunicationProtocol::Tcp, input_2_port)?;

    let audio_1_handle = audio_1_sender.send_non_blocking(audio_input_1);
    let audio_2_handle = audio_2_sender.send_non_blocking(audio_input_2);

    instance.send_request("start", json!({}))?;
    thread::sleep(Duration::from_secs(10));
    instance.send_request(
        "output/output_1/update",
        json!({
            "audio": {
                "inputs": [
                    {
                        "input_id": "input_1",
                        "volume": 0.5,
                    },
                    {
                        "input_id": "input_2",
                        "volume": 0.5,
                    },
                ]
           },
        }),
    )?;

    audio_1_handle.join().unwrap();
    audio_2_handle.join().unwrap();
    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        audio::ValidationMode::Artificial,
        AudioValidationConfig::default(),
    )?;

    Ok(())
}

#[pipeline_test(
    description = "
        Single audio input with a 440 Hz tone.

        Play audio for 20 seconds, the last few seconds should be silent.
    ",
    snapshot_name = "single_input_opus_output.rtp"
)]
pub fn single_input_opus() -> Result<()> {
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

    let audio_input_1 = input_dump_from_disk("a_opus_audio.rtp")?;
    PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?.send(&audio_input_1)?;

    instance.send_request("start", json!({}))?;

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        audio::ValidationMode::Artificial,
        AudioValidationConfig::default(),
    )?;

    Ok(())
}

#[pipeline_test(
    description = "
        An AAC audio input stream.

        Play audio for 10 seconds.
    ",
    snapshot_name = "single_input_aac_output.rtp"
)]
pub fn single_input_aac() -> Result<()> {
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

    let audio_validation_config = AudioValidationConfig {
        tolerance: AudioAnalyzeTolerance {
            frequency_tolerance: audio::FrequencyTolerance::Real(Default::default()),
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

#[pipeline_test(
    description = "
        Single mp4 audio input with a 440 Hz tone.

        Play audio for 10 seconds.
    ",
    snapshot_name = "single_input_aac_mp4_output.rtp"
)]
pub fn single_input_aac_mp4() -> Result<()> {
    let instance = CompositorInstance::start(None);
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

    let input_path = submodule_root_path()
        .join("rtp_packet_dumps")
        .join("inputs")
        .join("a_aac.mp4");

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "mp4",
            "path": input_path,
        }),
    )?;

    instance.send_request("start", json!({}))?;

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        audio::ValidationMode::Artificial,
        AudioValidationConfig::default(),
    )?;

    Ok(())
}

#[pipeline_test(
    description = "
        Single frequency input that changes after 5 seconds. Input starts streaming 2 seconds before
        start request with offset set to 2 seconds.

        Play 2 seconds of silence, 5 seconds of lower frequency and higher frequency after that time.
    ",
    snapshot_name = "audio_early_streaming_with_offset_output.rtp"
)]
fn audio_early_streaming_with_offset() -> Result<()> {
    let instance = CompositorInstance::start(None);
    let input_1_port = instance.get_port();
    let output_port = instance.get_port();

    let audio_input_1 = input_dump_from_disk("variable_frequency_opus_audio.rtp")?;

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
            "offset_ms": 2000,
        }),
    )?;

    let audio_sender = PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?;

    let audio_handle = audio_sender.send_non_blocking(audio_input_1);
    thread::sleep(Duration::from_secs(2));
    instance.send_request("start", json!({}))?;

    audio_handle.join().unwrap();

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        audio::ValidationMode::Artificial,
        AudioValidationConfig::default(),
    )?;

    Ok(())
}

#[pipeline_test(
    description = "
        Use input that changes frequency after 5 seconds. Input starts streaming 2 seconds before
        start request with no offset.

        Play approx. 3 seconds of lower frequency and higher frequency after that.
    ",
    snapshot_name = "audio_early_streaming_no_offset_output.rtp"
)]
fn audio_early_streaming_no_offset() -> Result<()> {
    let instance = CompositorInstance::start(None);
    let input_1_port = instance.get_port();
    let output_port = instance.get_port();

    let audio_input_1 = input_dump_from_disk("variable_frequency_opus_audio.rtp")?;

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
            "schedule_time_ms": 15000,
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

    let audio_sender = PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?;

    let audio_handle = audio_sender.send_non_blocking(audio_input_1);
    thread::sleep(Duration::from_secs(2));
    instance.send_request("start", json!({}))?;

    audio_handle.join().unwrap();

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        audio::ValidationMode::Artificial,
        AudioValidationConfig {
            tolerance: AudioAnalyzeTolerance {
                allowed_failed_batches: 2,
                ..Default::default()
            },
            ..Default::default()
        },
    )?;

    Ok(())
}
