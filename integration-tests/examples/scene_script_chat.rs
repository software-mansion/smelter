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

/// Chat on the right side of the screen, a new message shows up every second. See
/// `scene_script_chat.luau`.
fn client_code() -> Result<()> {
    download_all_samples()?;
    MediaReceiver::new(Receive::rtmp_listener(OUTPUT_PORT)).spawn()?;

    // The script does not know anything about the messages, they can have any length.
    let messages = [
        "hi",
        "Is the stream lagging for anyone else or is it just me?",
        "gg",
        "This message is deliberately long to check how the chat handles a text that needs many \
        lines. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor \
        incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud \
        exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.",
        "LOL",
        "Zażółć gęślą jaźń",
        "ok",
        "Can you show the settings again? I missed the part about the encoder preset.",
        "!",
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
                        "source": include_str!("./scene_script_chat.luau"),
                        "props": { "messages": messages },
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
