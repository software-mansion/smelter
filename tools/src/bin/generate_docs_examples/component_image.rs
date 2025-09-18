use std::{fs, path::Path, process::Command};

use anyhow::Result;
use serde_json::json;
use tools::compositor_instance::CompositorInstance;

use crate::examples_root;

pub(super) fn generate_image_component_example(root_dir: &Path) -> Result<()> {
    let instance = CompositorInstance::start();
    let mp4_path = root_dir.join("guides/component-image-example.mp4");
    let webp_path = root_dir.join("guides/component-image-example.webp");
    let _ = fs::remove_file(&mp4_path);

    instance.send_request(
        "image/image/register",
        json!({
            "asset_type": "svg",
            "path": examples_root().join("./src/bin/generate_docs_examples/image.svg"),
            "resolution": { "width": 915, "height": 720 }
        }),
    )?;

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "mp4",
            "path": mp4_path.to_str().unwrap(),
            "video": {
                "resolution": {
                    "width": 1280,
                    "height": 720,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                },
                "initial": scene()
            },
        }),
    )?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 1_000,
        }),
    )?;

    instance.send_request("start", json!({}))?;
    instance.wait_for_output_end();

    let _ = fs::remove_file(&webp_path);
    Command::new("ffmpeg")
        .args([
            "-i",
            mp4_path.to_str().unwrap(),
            "-vframes",
            "1",
            webp_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();

    let _ = fs::remove_file(&mp4_path);

    Ok(())
}

fn scene() -> serde_json::Value {
    json!({
        "root": {
            "type": "view",
            "background_color": "#52505b",
            "children": [
                {
                    "type": "image",
                    "image_id": "image",
                }
            ]
        }
    })
}
