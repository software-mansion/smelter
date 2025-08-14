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
        "output/output_1/register",
        &json!({
            "type": "mp4",
            "path": "output.mp4",
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
                        "id": "input_1",
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            },
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "aac",
                },
                "initial": {
                    "inputs": [
                        {"input_id": "input_1"}
                    ]
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "mp4",
            "url": BUNNY_URL,
            "offset_ms": 2000,
            "required": true,
        }),
    )?;
    examples::post("start", &json!({}))?;

    examples::post(
        "output/output_1/unregister",
        &json!({
            "schedule_time_ms": 50000
        }),
    )?;

    Ok(())
}
