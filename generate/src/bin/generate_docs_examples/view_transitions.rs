use std::{fs, process::Command, thread};

use anyhow::Result;
use generate::{compositor_instance::CompositorInstance, packet_sender::PacketSender};
use serde_json::json;

use crate::{pages_dir, workingdir};

pub(super) fn generate_view_transition_guide() -> Result<()> {
    generate_scene(
        "view_transition_1.webp",
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 480,
                    "child": { "type": "input_stream", "input_id": "input_1" },
                }
            ]
        }),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 1280,
                    "transition": {
                        "duration_ms": 2000,
                    },
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
            ]
        }),
    )?;

    generate_scene(
        "view_transition_2.webp",
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 480,
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "type": "rescaler",
                    "child": { "type": "input_stream", "input_id": "input_2" },
                }

            ]
        }),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 1280,
                    "transition": {
                        "duration_ms": 2000,
                    },
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "type": "rescaler",
                    "child": { "type": "input_stream", "input_id": "input_2" },
                }
            ]
        }),
    )?;

    generate_scene(
        "view_transition_3.webp",
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 480,
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
            ]
        }),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 1280,
                    "top": 0,
                    "left": 0,
                    "transition": { "duration_ms": 2000 },
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
            ]
        }),
    )?;

    generate_scene(
        "view_transition_4.webp",
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 0, "left": 0,
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "id": "rescaler_2",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 0, "left": 320,
                    "child": { "type": "input_stream", "input_id": "input_2" },
                },
                {
                    "id": "rescaler_3",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 0, "left": 640,
                    "child": { "type": "input_stream", "input_id": "input_3" },
                },
                {
                    "id": "rescaler_4",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 0, "left": 960,
                    "child": { "type": "input_stream", "input_id": "input_4" },
                },
            ]
        }),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 540, "left": 0,
                    "transition": { "duration_ms": 2000 },
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "id": "rescaler_2",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 540, "left": 320,
                    "transition": { "duration_ms": 2000, "easing_function": {"function_name": "bounce"} },
                    "child": { "type": "input_stream", "input_id": "input_2" },
                },
                {
                    "id": "rescaler_3",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 540, "left": 640,
                    "child": { "type": "input_stream", "input_id": "input_3" },
                    "transition": {
                        "duration_ms": 2000,
                        "easing_function": {
                            "function_name": "cubic_bezier",
                            "points": [0.65, 0, 0.35, 1]
                        }
                    },
                },
                {
                    "id": "rescaler_4",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 540, "left": 960,
                    "child": { "type": "input_stream", "input_id": "input_4" },
                    "transition": {
                        "duration_ms": 2000,
                        "easing_function": {
                            "function_name": "cubic_bezier",
                            "points": [0.33, 1, 0.68, 1]
                        }
                    },
                },
            ]
        }),
    )?;
    Ok(())
}

pub(super) fn generate_scene(
    filename: &str,
    scene_start: serde_json::Value,
    scene_change: serde_json::Value,
) -> Result<()> {
    let instance = CompositorInstance::start();
    let output_port = instance.get_port();
    let input_1_port = instance.get_port();
    let input_2_port = instance.get_port();
    let input_3_port = instance.get_port();
    let input_4_port = instance.get_port();

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
                "initial": {
                    "root": scene_start,
                },
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

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 8_000,
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": {
                "root": scene_change
            },
            "schedule_time_ms": 2000
        }),
    )?;

    let path = pages_dir().join("guides").join("assets").join(filename);
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
