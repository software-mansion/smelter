use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;
use std::env;

use integration_tests::{
    examples::{self, TestSample, run_example},
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_send},
};

const HTML_FILE_PATH: &str = "examples/web_view.html";

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const IP: &str = "127.0.0.1";
const INPUT_PORT: u16 = 8002;
const OUTPUT_PORT: u16 = 8004;

fn main() {
    unsafe {
        env::set_var("SMELTER_WEB_RENDERER_ENABLE", "1");
    }

    #[cfg(feature = "web-renderer")]
    {
        let target_path = &std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("..");
        if let Err(err) = libcef::bundle_for_development(target_path) {
            panic!(
                "Build process helper first. For release profile use: cargo build -r --bin process_helper. {err:?}"
            );
        }
    }

    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(Some(OUTPUT_PORT), None)?;

    let html_file_path = env::current_dir()?
        .join(HTML_FILE_PATH)
        .display()
        .to_string();

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_PORT,
            "video": {
                "decoder": "ffmpeg_h264"
            }
        }),
    )?;

    examples::post(
        "web-renderer/example_website/register",
        &json!({
            "url": format!("file://{html_file_path}"), // or other way of providing source
            "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_PORT,
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
                        "id": "embed_input_on_website",
                        "type": "web_view",
                        "instance_id": "example_website",
                        "children": [
                            {
                                "id": "big_bunny_video",
                                "type": "input_stream",
                                "input_id": "input_1",
                            }
                        ]
                    }
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    start_ffmpeg_send(IP, Some(INPUT_PORT), None, TestSample::SampleLoopH264)?;
    Ok(())
}
