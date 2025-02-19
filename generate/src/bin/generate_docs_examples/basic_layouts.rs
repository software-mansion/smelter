use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use generate::compositor_instance::CompositorInstance;
use serde_json::json;

use crate::workingdir;

pub(super) fn generate_basic_layouts_guide(root_dir: &Path) -> Result<()> {
    generate_scene(
        root_dir.join("guides/basic-layouts-1.mp4"),
        json!({
            "type": "view",
            "background_color": "#52505bff",
        }),
    )?;
    generate_scene(
        root_dir.join("guides/basic-layouts-2.mp4"),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                { "type": "input_stream", "input_id": "input_1" },
            ]
        }),
    )?;
    generate_scene(
        root_dir.join("guides/basic-layouts-3.mp4"),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "type": "rescaler",
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
            ]
        }),
    )?;
    generate_scene(
        root_dir.join("guides/basic-layouts-4.mp4"),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "type": "rescaler",
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "type": "rescaler",
                    "child": { "type": "input_stream", "input_id": "input_2" },
                }
            ]
        }),
    )?;
    generate_scene(
        root_dir.join("guides/basic-layouts-5.mp4"),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "type": "rescaler",
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "type": "rescaler",
                    "width": 320,
                    "height": 180,
                    "top": 20,
                    "right": 20,
                    "child": { "type": "input_stream", "input_id": "input_2" },
                }
            ]
        }),
    )?;
    Ok(())
}

pub(super) fn generate_scene(mp4_path: PathBuf, scene: serde_json::Value) -> Result<()> {
    let instance = CompositorInstance::start();

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
                    "preset": "ultrafast"
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
