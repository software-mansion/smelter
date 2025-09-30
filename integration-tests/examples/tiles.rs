use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;

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

    let scene_with_inputs = |n: usize| {
        let children: Vec<_> = (0..n)
            .map(|_| {
                json!({
                    "type": "input_stream",
                    "input_id": "input_1"
                })
            })
            .collect();
        json!({
            "type": "tiles",
            "id": "tile",
            "padding": 5,
            "background_color": "#444444FF",
            "children": children,
            "transition": {
                "duration_ms": 700,
                "easing_function": {
                    "function_name": "cubic_bezier",
                    "points": [0.35, 0.22, 0.1, 0.8]
                }
            },
        })
    };

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
                    "root": scene_with_inputs(0)
                }
            }
        }),
    )?;

    for i in 1..=16 {
        examples::post(
            "output/output_1/update",
            &json!({
                "video": {
                    "root": scene_with_inputs(i)
                },
                "schedule_time_ms": i * 1000,
            }),
        )?;
    }

    examples::post("start", &json!({}))?;

    start_ffmpeg_send(IP, Some(INPUT_PORT), None, TestSample::TestPatternH264)?;

    for i in 0..16 {
        examples::post(
            "output/output_1/update",
            &json!({
                "video": {
                    "root": scene_with_inputs(16 - i),
                },
                "schedule_time_ms": (20 + i) * 1000,
            }),
        )?;
    }

    examples::post(
        "output/output_1/update",
        &json!({
            "video": {
                "root": scene_with_inputs(4),
            },
            "schedule_time_ms": 40 * 1000,
        }),
    )?;

    Ok(())
}
