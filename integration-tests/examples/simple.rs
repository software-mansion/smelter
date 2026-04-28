use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;

use integration_tests::{
    examples::{self, run_example},
    media::{MediaReceiver, Receive, TestSample, download_all_samples},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const OUTPUT_PORT: u16 = 8004;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    download_all_samples()?;
    MediaReceiver::new(Receive::rtmp_listener(OUTPUT_PORT)).spawn()?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "mp4",
            "path": TestSample::BigBuckBunnyH264AAC.file()
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
            "type": "rtmp_client",
            "url": format!("rtmp://127.0.0.1:{OUTPUT_PORT}"),
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
                        "type": "shader",
                        "id": "shader_node_1",
                        "shader_id": "shader_example_1",
                        "children": [
                            {
                                "id": "input_1",
                                "type": "input_stream",
                                "input_id": "input_1",
                            }
                        ],
                        "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                    }
                }
            },
            "audio": {
                "initial": {
                    "inputs": [
                        {"input_id": "input_1"}
                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "aac",
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    Ok(())
}
