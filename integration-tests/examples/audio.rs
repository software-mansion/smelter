use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;
use std::time::Duration;

use integration_tests::{
    examples::{self, run_example, TestSample},
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_send},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const INPUT_1_PORT: u16 = 8002;
const INPUT_2_PORT: u16 = 8004;
const INPUT_3_PORT: u16 = 8006;
const INPUT_4_PORT: u16 = 8008;
const OUTPUT_VIDEO_PORT: u16 = 8010;
const OUTPUT_AUDIO_PORT: u16 = 8012;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(Some(OUTPUT_VIDEO_PORT), Some(OUTPUT_AUDIO_PORT))?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_1_PORT,
            "video": {
                "decoder": "ffmpeg_h264"
            },
        }),
    )?;

    examples::post(
        "input/input_2/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_2_PORT,
            "audio": {
                "decoder": "opus"
            },
        }),
    )?;

    examples::post(
        "input/input_3/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_3_PORT,
            "video": {
                "decoder": "ffmpeg_h264"
            },
        }),
    )?;

    examples::post(
        "input/input_4/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_4_PORT,
            "audio": {
                "decoder": "opus"
            },
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_VIDEO_PORT,
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "fast"
                },
                "initial": {
                    "root": {
                        "type": "tiles",
                        "children": [
                            {
                                "type": "input_stream",
                                "input_id": "input_1"
                            },
                            {
                                "type": "input_stream",
                                "input_id": "input_3"
                            }
                        ]
                    }
                },
                "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
            }
        }),
    )?;

    examples::post(
        "output/output_2/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_AUDIO_PORT,
            "audio": {
                "initial": {
                    "inputs": [
                        {"input_id": "input_2"},
                        {"input_id": "input_4"}
                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    start_ffmpeg_send(
        IP,
        Some(INPUT_1_PORT),
        Some(INPUT_2_PORT),
        TestSample::BigBuckBunnyH264Opus,
    )?;
    start_ffmpeg_send(
        IP,
        Some(INPUT_3_PORT),
        Some(INPUT_4_PORT),
        TestSample::ElephantsDreamH264Opus,
    )?;

    Ok(())
}
