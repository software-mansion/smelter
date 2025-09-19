use std::{fs, path::Path, process::Command};

use anyhow::Result;
use serde_json::json;
use tools::compositor_instance::CompositorInstance;

pub(super) fn generate_text_component_example(root_dir: &Path) -> Result<()> {
    let instance = CompositorInstance::start();
    let mp4_path = root_dir.join("guides/component-text-example.mp4");
    let webp_path = root_dir.join("guides/component-text-example.webp");
    let _ = fs::remove_file(&mp4_path);

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
            "padding": 100,
            "direction": "column",
            "children": [
                {
                    "type": "text",
                    "font_size": 72,
                    "color": "#a5baf0",
                    "weight": "bold",
                    "text": "Example text"
                },
                {
                    "type": "view",
                    "height": 30,
                },
                {
                    "type": "text",
                    "font_size": 30,
                    "line_height": 44,
                    "wrap": "word",
                    "width": 1000,
                    "color": "#a5baf0",
                    "text": "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nullam consequat lorem a quam bibendum, non gravida tortor ornare. Cras blandit facilisis erat. Integer porta ullamcorper mauris ac maximus. Donec sapien diam, porttitor nec interdum sit amet, eleifend at lectus."
                },
            ]
        }
    })
}
