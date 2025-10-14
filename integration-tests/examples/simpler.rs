use anyhow::Result;
use serde_json::json;
use std::time::Duration;

use integration_tests::examples::{self, run_example};

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "mp4",
            "path": "bunny.mp4",
            "decoder_map": {
                "h264": "ffmpeg_h264",
            },
        }),
    )?;

    let shader_source = include_str!("./silly.wgsl");
    examples::post(
        "shader/shader_example_1/register",
        &json!({
            "source": shader_source,
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "mp4",
            "path": "processed_bunny.mp4",
            "video": {
                "resolution": {
                    "width": 1920,
                    "height": 1080,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                },
            },
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;
    std::thread::sleep(Duration::from_secs(10));

    examples::post("output/output_1/unregister", &json!({}))?;

    std::thread::sleep(Duration::from_secs(2));

    Ok(())
}
