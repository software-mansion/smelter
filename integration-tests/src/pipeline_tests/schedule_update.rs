use std::time::Duration;

use crate::{
    CommunicationProtocol, CompositorInstance, OutputReceiver, PacketSender, compare_video_dumps,
    input_dump_from_disk, video::VideoValidationConfig,
};
use anyhow::Result;
use serde_json::json;

/// Schedules an output update.
///
/// Show `input_1` for 2 seconds.
/// Show `input_1` and `input_2` side by side (transition with animation) for 18 seconds.
#[test]
pub fn schedule_update() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "schedule_update_output.rtp";
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
                        "type": "tiles",
                        "id": "tiles_1",
                        "padding": 3,
                        "background_color": "#DDDDDDFF",
                        "transition": {
                            "duration_ms": 500,
                            "easing_function": {
                                "function_name": "bounce"
                            }
                        },
                        "children": [
                            {
                                "type": "input_stream",
                                "input_id": "input_1",
                            },
                        ],
                    }
                }
            },
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": {
                "root": {
                    "type": "tiles",
                    "id": "tiles_1",
                    "padding": 3,
                    "background_color": "#DDDDDDFF",
                    "transition": {
                        "duration_ms": 500,
                        "easing_function": {
                            "function_name": "bounce"
                        }
                    },
                    "children": [
                        {
                            "type": "input_stream",
                            "input_id": "input_1",
                        },
                        {
                            "type": "input_stream",
                            "input_id": "input_2",
                        },
                    ],
                }
            },
            "schedule_time_ms": 2000
        }),
    )?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20000,
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_1_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "required": true,
            "offset_ms": 0,
        }),
    )?;

    instance.send_request(
        "input/input_2/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_2_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "required": true,
            "offset_ms": 0,
        }),
    )?;

    let input_1_sender = PacketSender::new(CommunicationProtocol::Tcp, input_1_port)?;
    let input_2_sender = PacketSender::new(CommunicationProtocol::Tcp, input_2_port)?;
    let input_1_dump = input_dump_from_disk("8_colors_input_video.rtp")?;
    let input_2_dump = input_dump_from_disk("8_colors_input_reversed_video.rtp")?;

    let input_1_handle = input_1_sender.send_non_blocking(input_1_dump);
    let input_2_handle = input_2_sender.send_non_blocking(input_2_dump);

    instance.send_request("start", json!({}))?;

    input_1_handle.join().unwrap();
    input_2_handle.join().unwrap();
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
