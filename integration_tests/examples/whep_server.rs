use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::time::Duration;

use integration_tests::examples::{self, run_example};

const BUNNY_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "whip_server",
            "bearer_token": "example",
            "video": {
                "decoder_preferences": [
                    "ffmpeg_h264"
                ]
            }
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "whep_server",
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                    "ffmpeg_options": {
                        "tune": "zerolatency"
                    }
                },
                "initial": {
                    "root": {
                        "id": "input_1",
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            },
           // "audio": {
           //     "channels": "stereo",
           //     "encoder": { "type": "opus", "preset": "quality" },
           //     "initial": {
           //         "inputs": [
           //             {"input_id": "input_1"}
           //         ]
           //     }
           // }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    Ok(())
}
