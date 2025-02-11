use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;
use std::{process::Command, time::Duration};

use integration_tests::examples::{self, run_example};

const BUNNY_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const OUTPUT_URL: &str = "rtmp://a.rtmp.youtube.com/live2/appkey";

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    Command::new("ffplay")
        .args(["-listen", "1", OUTPUT_URL])
        .spawn()?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "mp4",
            "url": BUNNY_URL,
            "required": true,
            "offset_ms": 0,
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
            "type": "rtmp_stream",
            "url": OUTPUT_URL,
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
                        "type": "view",
                        "children": [
                            {
                                "type": "rescaler",
                                "width": VIDEO_RESOLUTION.width,
                                "height": VIDEO_RESOLUTION.height,
                                "top": 0,
                                "left": 0,
                                "child": {
                                    "type": "input_stream",
                                    "input_id": "input_1",
                                }
                            },
                            {
                                "type": "view",
                                "bottom": 0,
                                "left": 0,
                                "width": VIDEO_RESOLUTION.width,
                                "height": 100,
                                "background_color_rgba": "#00000088",
                                "children": [
                                    { "type": "view" },
                                    {
                                        "type": "text",
                                        "text": "LiveCompositor 😃😍",
                                        "font_size": 80,
                                        "color_rgba": "#40E0D0FF",
                                        "weight": "bold",
                                    },
                                    { "type": "view" }
                                ]
                            }
                        ]
                    }
                }
            },
            "audio": {
                "encoder": {
                    "type": "aac",
                    "channels": "stereo"
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
