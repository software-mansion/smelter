// TODO: #remove
use std::time::Duration;

use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;

use integration_tests::{
    examples::{self, TestSample, run_example},
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_send},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const IP: &str = "127.0.0.1";
const INPUT_PORT_1: u16 = 8002;
const INPUT_PORT_2: u16 = 8004;
const OUTPUT_PORT: u16 = 8006;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(Some(OUTPUT_PORT), None)?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_PORT_1,
            "video": {
                "decoder": "ffmpeg_h264"
            }
        }),
    )?;

    examples::post(
        "input/input_2/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_PORT_2,
            "video": {
                "decoder": "ffmpeg_h264"
            }
        }),
    )?;

    let scene_1 = json!({
        "type": "tiles",
        "id": "tile",
        "transition": {
            "duration_ms": 700,
        },
        "horizontal_align": "left",
        "children": [
            {
                "type": "input_stream",
                "input_id": "input_1",
                "id": "1"
            },
            {
                "type": "input_stream",
                "input_id": "input_1",
                "id": "2"
            },
            {
                "type": "input_stream",
                "input_id": "input_1",
                "id": "3"
            },
        ],
    });

    let scene_2 = json!({
        "type": "tiles",
        "id": "tile",
        "transition": {
            "duration_ms": 700,
        },
        "horizontal_align": "left",
        // "margin": 50,
        "children": [
            {
                "type": "input_stream",
                "input_id": "input_1",
                "id": "1"
            },
            {
                "type": "input_stream",
                "input_id": "input_1",
                "id": "2",
            },
            {
                "type": "input_stream",
                "input_id": "input_2",
                "id": "4",
            },
            {
                "type": "input_stream",
                "input_id": "input_2",
                "id": "5"
            },
        ],
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
                    "root": scene_1,
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    start_ffmpeg_send(IP, Some(INPUT_PORT_1), None, TestSample::TestPatternH264)?;
    start_ffmpeg_send(
        IP,
        Some(INPUT_PORT_2),
        None,
        TestSample::BigBuckBunnyH264Opus,
    )?;

    for _ in 0..15 {
        examples::post(
            "output/output_1/update",
            &json!({
                "video": {
                    "root": scene_2,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(3));
        examples::post(
            "output/output_1/update",
            &json!({
                "video": {
                    "root": scene_1,
                },
            }),
        )?;
        std::thread::sleep(Duration::from_secs(3));
    }

    Ok(())
}
