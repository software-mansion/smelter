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
const INPUT_PORT: u16 = 8002;
const OUTPUT_PORT: u16 = 8004;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    info!("[example] Start listening on output port.");
    start_ffmpeg_receive(Some(OUTPUT_PORT), None)?;

    info!("[example] Send register input request.");
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
    info!("[example] Register shader transform");
    examples::post(
        "shader/shader_example_1/register",
        &json!({
            "source": shader_source,
        }),
    )?;

    info!("[example] Send register output request.");
    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "port": OUTPUT_PORT,
            "ip": IP,
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
                    "root": {
                        "type": "shader",
                        "id": "shader_node_1",
                        "shader_id": "shader_example_1",
                        "children": [
                            {
                                "id": "input_1",
                                "type": "input_stream",
                                "input_id": "input_1",
                            }
                        ],
                        "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                    }
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    info!("[example] Start pipeline");
    examples::post("start", &json!({}))?;

    start_ffmpeg_send_sample(IP, Some(INPUT_PORT), None, TestSample::SampleLoop)?;
    Ok(())
}
