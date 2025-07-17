use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::info;

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_receive_h264,
};

const IP: &str = "127.0.0.1";
const INPUT_PORT_1: u16 = 8002;
const INPUT_PORT_2: u16 = 8004;
const INPUT_PORT_3: u16 = 8006;
const INPUT_PORT_4: u16 = 8008;
const INPUT_PORT_5: u16 = 8010;
const OUTPUT_PORT_1: u16 = 8012;

const PATH: &str = env!("CARGO_MANIFEST_DIR");

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(None, Some(OUTPUT_PORT_1))?;

    let path = Path::new(PATH).join("examples/assets/talkingHead.mp4");
    let path_quiet = Path::new(PATH).join("examples/assets/talkingHeadQuiet.mp4");

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 0,
        }),
    )?;

    examples::post(
        "input/input_2/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 5_000,
        }),
    )?;

    examples::post(
        "input/input_3/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 10_000,
        }),
    )?;

    examples::post(
        "input/input_4/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 15_000,
        }),
    )?;

    examples::post(
        "input/input_5/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 20_000,
        }),
    )?;

    examples::post(
        "input/input_6/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 25_000,
        }),
    )?;

    examples::post(
        "input/input_7/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 30_000,
        }),
    )?;

    examples::post(
        "input/input_8/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 35_000,
        }),
    )?;

    examples::post(
        "input/input_9/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 40_000,
        }),
    )?;

    examples::post(
        "input/input_10/register",
        &json!({
            "type": "mp4",
            "path": path,
            "offset_ms": 45_000,
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_PORT_1,
            "audio": {
                "channels": "stereo",
                "mixing_strategy": "sum_scale",
                "encoder": {
                    "type": "opus",
                    "sample_rate": 48000,
                    "preset": "quality",
                },
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_1",
                        },
                        {
                            "input_id": "input_2",
                        },
                        {
                            "input_id": "input_3",
                        },
                        {
                            "input_id": "input_4",
                        },
                        {
                            "input_id": "input_5",
                        },
                        {
                            "input_id": "input_6",
                        },
                        {
                            "input_id": "input_7",
                        },
                        {
                            "input_id": "input_8",
                        },
                        {
                            "input_id": "input_9",
                        },
                        {
                            "input_id": "input_10",
                        },
                    ],
                },
            },
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    std::thread::sleep(Duration::from_secs(5));

    Ok(())
}
