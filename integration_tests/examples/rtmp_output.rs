use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::{env, time::Duration};

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_rtmp_receive,
};

const BUNNY_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const OUTPUT_PORT: u16 = 9002;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    let url = match env::var("RTMP_URL") {
        Ok(url) => url,
        Err(_) => {
            start_ffmpeg_rtmp_receive(OUTPUT_PORT)?;
            std::thread::sleep(Duration::from_millis(2000));
            format!("rtmp://127.0.0.1:{OUTPUT_PORT}")
        }
    };

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
                        "type": "rescaler",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_1",
                        }
                    }
                }
            },
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "aac",
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

    examples::post("start", &json!({}))?;

    Ok(())
}
