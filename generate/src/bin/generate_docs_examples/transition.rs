use std::{fs, process::Command, thread};

use anyhow::Result;
use generate::{compositor_instance::CompositorInstance, packet_sender::PacketSender};
use serde_json::json;

use crate::{pages_dir, workingdir};

pub(super) fn generate_tile_transition_video() -> Result<()> {
    let instance = CompositorInstance::start();
    let output_port = instance.get_port();
    let input_1_port = instance.get_port();
    let input_2_port = instance.get_port();
    let input_3_port = instance.get_port();
    let input_4_port = instance.get_port();
    let input_5_port = instance.get_port();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": output_port,
            "video": {
                "resolution": {
                    "width": 1280,
                    "height": 720,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast"
                },
                "initial": scene(vec!["input_1", "input_2"])
            },
        }),
    )?;

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_1_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "required": true
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
            "required": true
        }),
    )?;

    instance.send_request(
        "input/input_3/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_3_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "offset_ms": 2500,
            "required": true
        }),
    )?;

    instance.send_request(
        "input/input_4/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_4_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "offset_ms": 4500,
            "required": true
        }),
    )?;

    instance.send_request(
        "input/input_5/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_5_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "offset_ms": 6500,
            "required": true
        }),
    )?;

    PacketSender::new(input_1_port)
        .unwrap()
        .send(&fs::read(workingdir().join("input_1.rtp")).unwrap())
        .unwrap();
    PacketSender::new(input_2_port)
        .unwrap()
        .send(&fs::read(workingdir().join("input_2.rtp")).unwrap())
        .unwrap();
    PacketSender::new(input_3_port)
        .unwrap()
        .send(&fs::read(workingdir().join("input_3.rtp")).unwrap())
        .unwrap();
    PacketSender::new(input_4_port)
        .unwrap()
        .send(&fs::read(workingdir().join("input_4.rtp")).unwrap())
        .unwrap();
    PacketSender::new(input_5_port)
        .unwrap()
        .send(&fs::read(workingdir().join("input_5.rtp")).unwrap())
        .unwrap();

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 14_000,
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_2", "input_3"]),
            "schedule_time_ms": 2000
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_2", "input_3", "input_4"]),
            "schedule_time_ms": 4000
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_2", "input_3", "input_4", "input_5"]),
            "schedule_time_ms": 6000
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_2", "input_3", "input_5", "input_4"]),
            "schedule_time_ms": 8000
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_2", "input_4"]),
            "schedule_time_ms": 10_000
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_2"]),
            "schedule_time_ms": 12_000
        }),
    )?;

    let path = pages_dir()
        .join("api")
        .join("components")
        .join("tile_transition.webp");
    let gst_thread = thread::Builder::new().name("gst sink".to_string()).spawn(move  ||{
        let gst_cmd = format!(
            "gst-launch-1.0 -v tcpclientsrc host=127.0.0.1 port={} ! \"application/x-rtp-stream\" ! rtpstreamdepay ! rtph264depay ! video/x-h264,framerate=30/1 ! h264parse ! h264timestamper ! decodebin ! webpenc animated=true speed=6 quality=50 ! filesink location={}",
            output_port,
            path.to_string_lossy(),
        );
        Command::new("bash").arg("-c").arg(gst_cmd).status().unwrap();
    }).unwrap();

    instance.send_request("start", json!({}))?;

    gst_thread.join().unwrap();

    Ok(())
}

fn scene(inputs: Vec<&str>) -> serde_json::Value {
    let inputs = inputs
        .into_iter()
        .map(|id| {
            json!({
                "type": "input_stream",
                "input_id": id,
                "id": id,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "root": {
            "type": "tiles",
            "id": "tile",
            "children": inputs,
            "margin": 20,
            "background_color": "#52505bff",
            "transition": {
                "duration_ms": 500,
                "easing_function": {
                    "function_name": "cubic_bezier",
                    "points": [0.35, 0.22, 0.1, 0.8]
                }
            },
        }
    })
}
