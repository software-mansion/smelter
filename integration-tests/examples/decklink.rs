use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_rtmp_receive,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const OUTPUT_PORT: u16 = 8002;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_rtmp_receive(OUTPUT_PORT)?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "decklink",
            "display_name": "DeckLink Quad HDMI Recorder (1)",
            "enable_audio": true,
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
                        "type": "view",
                        "background_color": "#4d4d4dff",
                        "children": [
                            {
                                "type": "rescaler",
                                "top": 10,
                                "left": 10,
                                "width": VIDEO_RESOLUTION.width - 20,
                                "height": VIDEO_RESOLUTION.height - 20,
                                "child": {
                                    "id": "input_1",
                                    "type": "input_stream",
                                    "input_id": "input_1",
                                }
                            }
                        ]
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
