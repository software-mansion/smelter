use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::time::Duration;

use integration_tests::examples::{self, run_example};

const BUNNY_URL: &str =
    "https://raw.githubusercontent.com/membraneframework/membrane_http_adaptive_stream_plugin/master/test/membrane_http_adaptive_stream/integration_test/fixtures/audio_multiple_video_tracks/index.m3u8";

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
            "type": "hls",
            "url": BUNNY_URL,
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "url": "rtmp://0.0.0.0:9002",
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

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    Ok(())
}
