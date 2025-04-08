use anyhow::{anyhow, Result};
use compositor_api::types::Resolution;
use serde_json::json;
use std::{env, time::Duration};

use integration_tests::examples::{self, run_example};

const BUNNY_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";

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
            "type": "mp4",
            "url": BUNNY_URL,
            "required": true,
            "offset_ms": 0,
        }),
    )?;

    let token = env::var("SMELTER_WHIP_CLIENT_EXAMPLE_TOKEN").map_err(|err| anyhow!("Couldn't read SMELTER_WHIP_CLIENT_EXAMPLE_TOKEN environmental variable. You must provide it in order to run `whip_client` example. Read env error: {}", err))?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "whip",
            "endpoint_url": "https://whip.vdo.ninja", // Twitch WHIP endpoint URL
            "bearer_token": token,
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_vp8",
                },
                "initial": {
                    "root": {
                        "id": "input_1",
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            },
            "audio": {
                "encoder": {
                    "type": "opus",
                    "channels": "stereo",
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
