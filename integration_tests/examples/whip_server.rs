use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::{thread::sleep, time::Duration};

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_rtmp_receive,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const OUTPUT_PORT: u16 = 9002;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_rtmp_receive(OUTPUT_PORT)?;
    std::thread::sleep(Duration::from_millis(2000));

    examples::post(
        "input/input/register",
        &json!({
            "type": "whip_server",
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
            "url": format!("rtmp://127.0.0.1:{OUTPUT_PORT}"),
            "type": "rtmp_client",
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                },
                "initial": {
                    "root": {
                        "type": "tiles",
                        "background_color": "#4d4d4dff",
                        "children": [
                            {
                                "type": "rescaler",
                                "child": { "type": "input_stream", "input_id": "input" }
                            }
                        ]
                    }
                },

            },
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "aac",
                    "sample_rate": 48_000
                },
                "initial": {
                    "inputs": [
                        {"input_id": "input"}
                    ]
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));
    examples::post("start", &json!({}))?;
    sleep(Duration::MAX);

    Ok(())
}
