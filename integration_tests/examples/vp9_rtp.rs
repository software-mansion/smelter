use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;
use std::{thread::sleep, time::Duration};

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_send,
    gstreamer::start_gst_receive_tcp_vp9,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const IP: &str = "127.0.0.1";
const INPUT_1_PORT: u16 = 8002;
const INPUT_2_PORT: u16 = 8003;
const OUTPUT_PORT: u16 = 8004;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_1_PORT,
            "video": {
                "decoder": "ffmpeg_vp9"
            }
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
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "port": OUTPUT_PORT,
            "transport_protocol": "tcp_server",
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_vp9",
                    "pixel_format": "yuv444p"
                },
                "initial": {
                    "root": {
                        "type": "view",
                        "background_color": "#4d4d4dff",
                        "children": [
                            {
                              "type": "rescaler",
                              "width": VIDEO_RESOLUTION.width,
                              "height": VIDEO_RESOLUTION.height,
                              "child": {
                                "type": "input_stream",
                                "input_id": "input_1"
                              }
                            }
                        ]
                    }
                },

            },
            "audio": {
                "initial": {
                    "inputs": [
                        {"input_id": "input_2"},
                    ]
                },
                "channels": "stereo",
                "encoder": {
                   "type": "opus",
                }
            },
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));
    start_gst_receive_tcp_vp9(IP, OUTPUT_PORT, true)?;
    examples::post("start", &json!({}))?;

    start_ffmpeg_send(
        IP,
        Some(INPUT_1_PORT),
        Some(INPUT_2_PORT),
        examples::TestSample::BigBuckBunnyVP9Opus,
    )?;

    sleep(Duration::from_secs(300));
    examples::post("output/output_1/unregister", &json!({}))?;

    Ok(())
}
