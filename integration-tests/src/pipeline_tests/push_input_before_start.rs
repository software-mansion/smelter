use std::{thread, time::Duration};

use anyhow::Result;
use serde_json::json;

use crate::{
    CommunicationProtocol, CompositorInstance, OutputReceiver, PacketSender, compare_video_dumps,
    input_dump_from_disk, video::VideoValidationConfig,
};

/// Check if the input stream is passed to the output correctly even if entire
/// stream was delivered before the compositor start. (TCP input)
///
/// Output:
/// - Display entire input stream from the beginning (16 seconds). No black frames at the
///   beginning. Starts with a green color.
/// - Black screen for remaining 4 seconds.
#[test]
pub fn push_input_before_start_tcp() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "push_entire_input_before_start_tcp.rtp";
    let instance = CompositorInstance::start(None);
    let input_port = instance.get_port();
    let output_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "video": {
                "resolution": {
                    "width": 640,
                    "height": 360,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast"
                },
                "initial": {
                    "root": {
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            },
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20_000,
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
            "required": true,
            "offset_ms": 0
        }),
    )?;

    let input_1_dump = input_dump_from_disk("8_colors_input_video.rtp")?;
    let input_1_handle =
        PacketSender::new(CommunicationProtocol::Tcp, input_port)?.send_non_blocking(input_1_dump);

    thread::sleep(Duration::from_secs(5));

    instance.send_request("start", json!({}))?;

    input_1_handle.join().unwrap();
    let new_output_dump = output_receiver.wait_for_output()?;

    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        VideoValidationConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_secs(20)],
            ..Default::default()
        },
    )?;

    Ok(())
}

/// Check if the input stream is passed to the output correctly even if entire
/// stream was delivered before the compositor start. (UDP)
///
/// Output:
/// - Display entire input stream from the beginning (16 seconds). No black frames at the
///   beginning. Starts with a green screen.
/// - Black screen for remaining 4 seconds.
#[test]
pub fn push_input_before_start_udp() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "push_entire_input_before_start_udp.rtp";
    let instance = CompositorInstance::start(None);
    let input_port = instance.get_port();
    let output_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "video": {
                "resolution": {
                    "width": 640,
                    "height": 360,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast"
                },
                "initial": {
                    "root": {
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            },
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20_000,
        }),
    )?;

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": input_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "required": true,
            "offset_ms": 0
        }),
    )?;

    let input_1_dump = input_dump_from_disk("8_colors_input_video.rtp")?;
    let input_1_handle =
        PacketSender::new(CommunicationProtocol::Udp, input_port)?.send_non_blocking(input_1_dump);

    thread::sleep(Duration::from_secs(5));

    instance.send_request("start", json!({}))?;

    input_1_handle.join().unwrap();
    let new_output_dump = output_receiver.wait_for_output()?;

    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        VideoValidationConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_secs(20)],
            ..Default::default()
        },
    )?;

    Ok(())
}

/// Check if the input stream is processed correctly if the stream is delivered few seconds before
/// queue start. Test case where there is no offset defined. (TCP server)
///
/// Output:
/// - Display input stream without initial 5 seconds from the beginning (11 seconds). Not black frames at the
///   beginning. Starts with a red color. Initial 5 second of input stream is missing.
/// - Black screen for remaining 9 seconds.
#[test]
pub fn push_input_before_start_tcp_no_offset() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "push_entire_input_before_start_tcp_without_offset.rtp";
    let instance = CompositorInstance::start(None);
    let input_port = instance.get_port();
    let output_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "video": {
                "resolution": {
                    "width": 640,
                    "height": 360,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast"
                },
                "initial": {
                    "root": {
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            },
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20_000,
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
            "required": true,
        }),
    )?;

    let input_1_dump = input_dump_from_disk("8_colors_input_video.rtp")?;
    let input_1_handle =
        PacketSender::new(CommunicationProtocol::Tcp, input_port)?.send_non_blocking(input_1_dump);

    thread::sleep(Duration::from_secs(5));

    instance.send_request("start", json!({}))?;

    input_1_handle.join().unwrap();
    let new_output_dump = output_receiver.wait_for_output()?;

    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        VideoValidationConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_secs(20)],
            allowed_invalid_frames: 10,
            ..Default::default()
        },
    )?;

    Ok(())
}

/// Check if the input stream is processed correctly if the stream is delivered few seconds before
/// queue start. Test case where there is no offset defined. (UPD)
///
/// Output:
/// - Display entire input stream from the beginning (16 seconds). No black frames at the
///   beginning. Starts with a red color. Initial 5 second of input stream is missing.
/// - Black screen for remaining 19 seconds.
#[test]
pub fn push_input_before_start_udp_no_offset() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "push_entire_input_before_start_udp_without_offset.rtp";
    let instance = CompositorInstance::start(None);
    let input_port = instance.get_port();
    let output_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "video": {
                "resolution": {
                    "width": 640,
                    "height": 360,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast"
                },
                "initial": {
                    "root": {
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            },
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20_000,
        }),
    )?;

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": input_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "required": true,
        }),
    )?;

    let input_1_dump = input_dump_from_disk("8_colors_input_video.rtp")?;
    let input_1_handle =
        PacketSender::new(CommunicationProtocol::Udp, input_port)?.send_non_blocking(input_1_dump);

    thread::sleep(Duration::from_secs(5));

    instance.send_request("start", json!({}))?;

    input_1_handle.join().unwrap();
    let new_output_dump = output_receiver.wait_for_output()?;

    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        VideoValidationConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_secs(18)],
            allowed_invalid_frames: 10,
            ..Default::default()
        },
    )?;

    Ok(())
}
