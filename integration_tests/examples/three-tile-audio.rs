use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::path::Path;
use std::time::Duration;

use integration_tests::examples::{self, run_example};

const PATH: &str = env!("CARGO_MANIFEST_DIR");

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "mp4",
            "path": Path::new(PATH).join("examples/assets/lachrymaQuiet30s.mp4"),
            "video_decoder": "ffmpeg_h264",
        }),
    )?;

    examples::post(
        "input/input_2/register",
        &json!({
            "type": "mp4",
            "path": Path::new(PATH).join("examples/assets/peacefieldQuiet30s.mp4"),
            "video_decoder": "ffmpeg_h264",
        }),
    )?;

    examples::post(
        "input/input_3/register",
        &json!({
            "type": "mp4",
            "path": Path::new(PATH).join("examples/assets/satanizedQuiet30s.mp4"),
            "video_decoder": "ffmpeg_h264",
        }),
    )?;

    examples::post(
        "output/output_mp4/register",
        &json!({
            "type": "mp4",
            "path": Path::new(PATH).join("examples/assets/ghostMerged.mp4"),
            "video": {
                "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                },
                "initial": {
                    "root": {
                        "type": "tiles",
                        "children": [
                            {
                                "type": "input_stream",
                                "input_id": "input_1"
                            },
                            {
                                "type": "input_stream",
                                "input_id": "input_2"
                            },
                            {
                                "type": "input_stream",
                                "input_id": "input_3",
                            },
                        ],
                    },
                },
                "send_eos_when": {
                    "all_inputs": true,
                }
            },
            "audio": {
                "encoder": {
                    "type": "aac",
                    "channels": "stereo",
                    "sample_rate": 44100,
                },
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_1",
                        },
                        {
                            "input_id": "input_2",
                        },
                        {
                            "input_id": "input_3",
                            "volume": 0.5f32,
                        },
                    ],
                },
                "send_eos_when": {
                    "all_inputs": true,
                },
            },
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    Ok(())
}
