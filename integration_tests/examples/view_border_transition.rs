use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::{
    thread::{self},
    time::Duration,
};

use integration_tests::{
    examples::{self, run_example, TestSample},
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_send},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
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

    examples::post(
        "image/example_image/register",
        &json!({
            "url": "https://gifdb.com/images/high/rust-logo-on-fire-o41c0v9om8drr8dv.gif",
        }),
    )?;

    let scene1 = json!({
        "type": "view",
        "background_color": "#42daf5ff",
        "children": [
            {
                "type": "view",
                "id": "resized",
                "width": VIDEO_RESOLUTION.width,
                "height": VIDEO_RESOLUTION.height,
                "top": 0.0,
                "right": 0.0,
                "background_color": "#0000FFFF",
                "border_color": "#FFFFFFFF",
                "box_shadow": [
                    {
                        "offset_y": 40,
                        "offset_x": 0,
                        "blur_radius": 40,
                        "color": "#00000088",
                    }
                ],
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fill",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_1"
                        }
                    }
                ]
            }
        ]
    });

    let scene2 = json!({
        "type": "view",
        "background_color": "#42daf5ff",
        "children": [
            {
                "type": "view",
                "id": "resized",
                "width": 300,
                "height": 300,
                "top": (VIDEO_RESOLUTION.height as f32 - 330.0) / 2.0 ,
                "right": (VIDEO_RESOLUTION.width as f32 - 330.0) / 2.0,
                "border_radius": 50,
                "border_width": 15,
                "background_color": "#0000FFFF",
                "border_color": "#FFFFFFFF",
                "box_shadow": [
                    {
                        "offset_y": 40,
                        "offset_x": 0,
                        "blur_radius": 40,
                        "color": "#00000088",
                    }
                ],
                "transition": {
                    "duration_ms": 1500,
                    "easing_function": {
                        "function_name": "cubic_bezier",
                        "points": [0.33, 1, 0.68, 1]
                    }
                },
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fill",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_1"
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

    Ok(())
}
