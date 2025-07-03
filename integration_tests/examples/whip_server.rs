use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::{thread::sleep, time::Duration};

use integration_tests::{
    examples::{self, run_example},
    gstreamer::start_gst_receive_tcp_vp8,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const OUTPUT_PORT: u16 = 8012;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "whip",
            "bearer_token": "example",
            "video": {
                "decoder_preferences": [
                    "ffmpeg_vp9"
                ]
            }
        }),
    )?
    .json::<serde_json::Value>()?;

    examples::post(
        "input/input_2/register",
        &json!({
            "type": "whip",
            "bearer_token": "example",
            "video": {
                "decoder_preferences": [
                    "ffmpeg_vp8",
                    "ffmpeg_h264",
                    "any",
                ]
            },
        }),
    )?
    .json::<serde_json::Value>()?;

    examples::post(
        "input/input_3/register",
        &json!({
            "type": "whip",
            "bearer_token": "example",
            "video": {
                "decoder_preferences": [
                    "ffmpeg_h264"
                ]
            }
        }),
    )?
    .json::<serde_json::Value>()?;

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
                    "type": "ffmpeg_vp8",
                },
                "initial": {
                    "root": {
                        "type": "tiles",
                        "background_color": "#4d4d4dff",
                        "children": [
                            {
                                "type": "rescaler",
                                "child": { "type": "input_stream", "input_id": "input_1" }
                            },
                            {
                                "type": "rescaler",
                                "child": { "type": "input_stream", "input_id": "input_2" }
                            },
                            {
                                "type": "rescaler",
                                "child": { "type": "input_stream", "input_id": "input_3" }
                            }
                        ]
                    }
                },

            },
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                },
                "initial": {
                    "inputs": [
                        {"input_id": "input_1"},
                        {"input_id": "input_2"},
                        {"input_id": "input_3"}
                    ]
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));
    start_gst_receive_tcp_vp8(IP, OUTPUT_PORT, true)?;
    examples::post("start", &json!({}))?;
    sleep(Duration::MAX);

    Ok(())
}
