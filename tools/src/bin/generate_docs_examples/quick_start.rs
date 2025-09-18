use std::{fs, path::Path};

use anyhow::Result;
use serde_json::json;
use tools::compositor_instance::CompositorInstance;

use crate::workingdir;

pub(super) fn generate_quick_start_guide(root_dir: &Path) -> Result<()> {
    //generate_scene(
    //    root_dir.join("guides/quick-start-1.webp").to_str().unwrap(),
    //    json!({
    //        "type": "view",
    //        "background_color": "#52505bff",
    //        "children": []
    //    }),
    //)?;
    generate_scene(
        &root_dir.join("guides/quick-start.mp4"),
        json!({
            "type": "tiles",
            "background_color": "#52505bff",
            "children": [
                { "type": "input_stream", "input_id": "input_1" },
                { "type": "input_stream", "input_id": "input_2" },
            ]
        }),
    )?;

    Ok(())
}

pub(super) fn generate_scene(mp4_path: &Path, scene: serde_json::Value) -> Result<()> {
    let instance = CompositorInstance::start();

    let _ = fs::remove_file(mp4_path);

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
                "initial": {
                    "root": scene,
                },
            },
        }),
    )?;

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("input_1.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 0
        }),
    )?;

    instance.send_request(
        "input/input_2/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("input_2.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 0
        }),
    )?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 10_000,
        }),
    )?;

    instance.send_request("start", json!({}))?;
    instance.wait_for_output_end();

    Ok(())
}
