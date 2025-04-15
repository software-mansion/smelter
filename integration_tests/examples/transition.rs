use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;
use std::{thread, time::Duration};

use integration_tests::{
    examples::{self, run_example, TestSample},
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_send},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const IP: &str = "127.0.0.1";
const INPUT_PORT: u16 = 8002;
const OUTPUT_PORT: u16 = 8004;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(Some(OUTPUT_PORT), None)?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_PORT,
            "video": {
                "decoder": "ffmpeg_h264"
            }
        }),
    )?;

    let shader_source = include_str!("./silly.wgsl");
    examples::post(
        "shader/example_shader/register",
        &json!({
            "source": shader_source,
        }),
    )?;

    examples::post(
        "image/example_image/register",
        &json!({
            "asset_type": "gif",
            "url": "https://gifdb.com/images/high/rust-logo-on-fire-o41c0v9om8drr8dv.gif",
        }),
    )?;

    let scene1 = json!({
        "type": "view",
        "background_color": "#444444FF",
        "children": [
            {
                "type": "view",
                "id": "resized",
                "overflow": "fit",
                "width": 480,
                "height": 270,
                "top": 100,
                "right": 100,
                "children": [
                    {
                        "type": "shader",
                        "shader_id": "example_shader",
                        "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                        "children": [{
                            "type": "input_stream",
                            "input_id": "input_1",
                        }]
                    }
                ]
            }
        ]
    });

    let scene2 = json!({
        "type": "view",
        "background_color": "#444444FF",
        "children": [
            {
                "type": "view",
                "id": "resized",
                "width": VIDEO_RESOLUTION.width,
                "height": VIDEO_RESOLUTION.height,
                "top": 0,
                "right": 0,
                "transition": {
                    "duration_ms": 10000
                },
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fit",
                        "child": {
                            "type": "shader",
                            "shader_id": "example_shader",
                            "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                            "children": [{
                                "type": "input_stream",
                                "input_id": "input_1",
                            }]
                        }

                    }
                ]
            }
        ]
    });

    let scene3 = json!({
        "type": "view",
        "background_color": "#444444FF",
        "children": [
            {
                "type": "view",
                "id": "resized",
                "width": VIDEO_RESOLUTION.width,
                "height": VIDEO_RESOLUTION.height,
                "top": 0,
                "right": 0,
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fit",
                        "child": {
                            "type": "shader",
                            "shader_id": "example_shader",
                            "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                            "children": [{
                                "type": "input_stream",
                                "input_id": "input_1",
                            }]
                        }

                    }
                ]
            }
        ]
    });

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_PORT,
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast"
                },
                "initial": {
                    "root": scene1
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    start_ffmpeg_send(IP, Some(INPUT_PORT), None, TestSample::TestPatternH264)?;

    thread::sleep(Duration::from_secs(5));

    examples::post(
        "output/output_1/update",
        &json!({
            "video": {
                "root": scene2,
            }
        }),
    )?;

    thread::sleep(Duration::from_secs(2));

    examples::post(
        "output/output_1/update",
        &json!({
            "video": {
                "root": scene3
            },
        }),
    )?;

    Ok(())
}
