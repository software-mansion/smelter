use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;
use std::{path::PathBuf, time::Duration};

use integration_tests::{
    examples::{self, TestSample, run_example},
    ffmpeg::{Video, start_ffmpeg_receive_h264, start_ffmpeg_send, start_ffmpeg_send_from_file},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const OUTPUT_PORT: u16 = 8004;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(Some(OUTPUT_PORT), None)?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "mp4",
            "path": "./bunny.mp4",
            "decoder_map": {
                "h264": "ffmpeg_h264",
            },
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "mp4",
            "path": "./processed_bunny.mp4",
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
                                "id": "input_1",
                                "type": "input_stream",
                                "input_id": "input_1",
                            }
                        ],
                    }
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;
    std::thread::sleep(Duration::from_secs(10));
    examples::post("output/output_1/unregister", &json!({}))?;

    Ok(())
}
