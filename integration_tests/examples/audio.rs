use anyhow::Result;
use live_compositor::types::Resolution;
use log::info;
use serde_json::json;
use std::time::Duration;

use integration_tests::{
    examples::{self, run_example, TestSample},
    ffmpeg::{start_ffmpeg_receive, start_ffmpeg_send_sample},
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
    info!("[example] Start listening on output port.");
    start_ffmpeg_receive(Some(OUTPUT_VIDEO_PORT), Some(OUTPUT_AUDIO_PORT))?;

    info!("[example] Send register input request.");
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

    info!("[example] Send register input request.");
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

    info!("[example] Send register input request.");
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

    info!("[example] Send register input request.");
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

    info!("[example] Send register output request.");
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

    info!("[example] Send register output request.");
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
                "encoder": {
                    "type": "opus",
                    "channels": "stereo",
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    info!("[example] Start pipeline");
    examples::post("start", &json!({}))?;

    start_ffmpeg_send_sample(
        IP,
        Some(INPUT_1_PORT),
        Some(INPUT_2_PORT),
        TestSample::BigBuckBunny,
    )?;
    start_ffmpeg_send_sample(
        IP,
        Some(INPUT_3_PORT),
        Some(INPUT_4_PORT),
        TestSample::ElephantsDream,
    )?;

    Ok(())
}
