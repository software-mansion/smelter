use std::{thread, time::Duration};

use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;

use integration_tests::{
    examples::{self, run_example},
    media::{MediaReceiver, Receive, TestSample, download_all_samples},
    paths::integration_tests_root,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const OUTPUT_PORT: u16 = 8004;

fn main() {
    run_example(client_code);
}

/// Scene is defined by a Lua script. Requests below only register/unregister inputs and update
/// props, the script decides what to render when that happens.
fn client_code() -> Result<()> {
    download_all_samples()?;
    MediaReceiver::new(Receive::rtmp_listener(OUTPUT_PORT)).spawn()?;

    examples::post(
        "image/logo/register",
        &json!({
            "asset_type": "svg",
            "path": integration_tests_root().join("examples/assets/rust.svg"),
        }),
    )?;

    let script = include_str!("./scene_script.luau");
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
                        "source": script,
                        "props": { "title": "Scene defined by a Lua script" },
                    }
                }
            },
            "audio": {
                "encoder": { "type": "aac" },
                "initial": { "inputs": [] }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    let inputs = [
        ("big_buck_bunny", TestSample::BigBuckBunnyH264AAC),
        ("elephants_dream", TestSample::ElephantsDreamH264Opus),
        // The shortest one, the script removes it from the scene when it ends.
        ("ocean", TestSample::OceanSampleH264),
    ];
    for (input_id, sample) in inputs {
        thread::sleep(Duration::from_secs(5));
        examples::post(
            &format!("input/{input_id}/register"),
            &json!({ "type": "mp4", "path": sample.path() }),
        )?;
    }

    // The same source, so the script is not restarted, it only receives new props.
    thread::sleep(Duration::from_secs(5));
    examples::post(
        "output/output_1/update",
        &json!({
            "video": {
                "script": {
                    "source": script,
                    "props": { "title": "Props updated" },
                }
            },
            "audio": { "inputs": [] }
        }),
    )?;

    thread::sleep(Duration::from_secs(5));
    examples::post("input/big_buck_bunny/unregister", &json!({}))?;

    Ok(())
}
