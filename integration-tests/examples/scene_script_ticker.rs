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

/// Bar at the bottom of the screen with texts that move from right to left in a loop. See
/// `scene_script_ticker.luau`.
fn client_code() -> Result<()> {
    download_all_samples()?;
    MediaReceiver::new(Receive::rtmp_listener(OUTPUT_PORT)).spawn()?;

    // The script does not know anything about the texts, they can have any length.
    let texts = [
        "Hi",
        "Scene scripts react to inputs and timers in sync with rendered frames",
        "x",
        "This one is a lot longer than the screen. Lorem ipsum dolor sit amet, consectetur \
        adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut \
        enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea \
        commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
        cillum dolore eu fugiat nulla pariatur.",
        "Zażółć gęślą jaźń",
    ];

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
                    "ffmpeg_options": { "preset": "ultrafast" }
                },
                "initial": {
                    "script": {
                        "source": include_str!("./scene_script_ticker.luau"),
                        "props": { "texts": texts },
                    }
                }
            },
        }),
    )?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "mp4",
            "path": TestSample::BigBuckBunnyH264AAC.path(),
        }),
    )?;

    examples::post("start", &json!({}))?;

    Ok(())
}
