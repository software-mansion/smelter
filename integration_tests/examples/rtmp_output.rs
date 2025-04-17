use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;
use std::{env, time::Duration};

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

    let url = env::var("RTMP_URL").expect("Set RTMP_URL env variable");
    examples::post(
        "output/output_1/register",
        &json!({
            "url": url,
            "type": "rtmp_client",
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                    "ffmpeg_options": {
                        "g": "120", // keyframe every 100 frames
                        "b": "6M"   // bitrate 6000 kb/s
                    }
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
