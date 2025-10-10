use anyhow::{Result, anyhow};
use serde_json::json;
use smelter_api::Resolution;
use std::{env, thread::sleep, time::Duration};

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_rtmp_receive,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const OUTPUT_PORT: u16 = 9002;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_rtmp_receive(OUTPUT_PORT)?;
    std::thread::sleep(Duration::from_millis(2000));

    let endpoint_url = env::var("INPUT_URL").map_err(|err| anyhow!("Couldn't read INPUT_URL environmental variable. You must provide it in order to run `whep_client` example. Read env error: {}", err))?;
    let token = env::var("WHEP_TOKEN").map_err(|err| anyhow!("Couldn't read WHEP_TOKEN environmental variable. You must provide it in order to run `whep_client` example. Read env error: {}", err))?;

    examples::post(
        "input/input/register",
        &json!({
            "type": "whep_client",
            "endpoint_url": endpoint_url,
            "bearer_token": token,
            "video": {
                "decoder_preferences": [
                    "ffmpeg_vp9",
                    "ffmpeg_vp8",
                    "ffmpeg_h264",
                ]
            }
        }),
    )?
    .json::<serde_json::Value>()?;

    examples::post(
        "output/output_1/register",
        &json!({
            "url": format!("rtmp://127.0.0.1:{OUTPUT_PORT}"),
            "type": "rtmp_client",
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                },
                "initial": {
                    "root": {
                        "type": "tiles",
                        "background_color": "#4d4d4dff",
                        "children": [
                            {
                                "type": "rescaler",
                                "child": { "type": "input_stream", "input_id": "input" }
                            }
                        ]
                    }
                },
            },
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "aac",
                    "sample_rate": 48_000
                },
                "initial": {
                    "inputs": [
                        {"input_id": "input"}
                    ]
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));
    examples::post("start", &json!({}))?;
    sleep(Duration::MAX);

    Ok(())
}
