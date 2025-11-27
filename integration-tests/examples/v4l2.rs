use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_receive_h264,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const OUTPUT_PORT: u16 = 8002;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(Some(OUTPUT_PORT), None)?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "v4l2",
            "format": "yuyv",
            "resolution": {
                "width": VIDEO_RESOLUTION.width,
                "height": VIDEO_RESOLUTION.height,
            },
            "path": "/dev/video0",
            "framerate": 30,
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "whep_server",
            "bearer_token": "example",
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                    "ffmpeg_options": {
                        "tune": "zerolatency",
                        "thread_type": "slice",
                    },
                },
                "initial": {
                    "root": {
                        "type": "input_stream",
                        "input_id": "input_1"
                    },
                }
            },
            "audio": {
                "encoder": {
                    "type": "opus"
                },
                "initial": {
                    "inputs": [],
                }
            }
        }),
    )?;

    // file:///home/jerzywilczek/Repos/live-compositor/integration-tests/examples/demo/whep.html?url=http://127.0.0.1:9000/whep/output_1&token=example
    let url = format!(
        "file://{}/examples/demo/whep.html?url=http://127.0.0.1:9000/whep/output_1&token=example",
        env!("CARGO_MANIFEST_DIR")
    );
    println!("Visit this URL to watch the stream:\n{url}");

    examples::post("start", &json!({}))?;

    Ok(())
}
