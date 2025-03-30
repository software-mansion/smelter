use anyhow::Result;
use compositor_api::types::Resolution;
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
            "type": "mp4",
            "url": BUNNY_URL,
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtmp_client",
            "url": "rtmp://x.rtmp.youtube.com/live2/",
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
                        "type": "view",
                        "background_color": "red",
                        "children": [{
                            "type": "input_stream",
                            "input_id": "input_1",
                        }]
                    }
                }
            },
            "audio": {
                "encoder": {
                    "type": "aac",
                    "channels": "stereo",
                    "sample_rate": 44100
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

    examples::post("start", &json!({}))?;

    Ok(())
}
