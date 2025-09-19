use std::{fs, path::Path};

use anyhow::Result;
use serde_json::json;
use tools::compositor_instance::CompositorInstance;

use crate::workingdir;

pub(super) fn generate_tile_component_example(root_dir: &Path) -> Result<()> {
    let instance = CompositorInstance::start();
    let mp4_path = root_dir.join("guides/component-tiles-example.mp4");
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
                "initial": scene(vec!["input_1", "input_2"])
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
        "input/input_3/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("input_3.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 3000
        }),
    )?;

    instance.send_request(
        "input/input_4/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("input_4.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 6000
        }),
    )?;

    instance.send_request(
        "input/input_5/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("input_5.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 6000
        }),
    )?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 15_000,
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_2", "input_3"]),
            "schedule_time_ms": 3000
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_2", "input_3", "input_4", "input_5"]),
            "schedule_time_ms": 6000
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_3", "input_4", "input_5"]),
            "schedule_time_ms": 9_000
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(vec!["input_1", "input_4", "input_5"]),
            "schedule_time_ms": 12_000
        }),
    )?;

    instance.send_request("start", json!({}))?;
    instance.wait_for_output_end();

    Ok(())
}

fn scene(inputs: Vec<&str>) -> serde_json::Value {
    let inputs = inputs
        .into_iter()
        .map(|id| {
            json!({
                "type": "input_stream",
                "input_id": id,
                "id": id,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "root": {
            "type": "tiles",
            "id": "tile",
            "children": inputs,
            "margin": 20,
            "background_color": "#52505bff",
            "transition": {
                "duration_ms": 300,
                "easing_function": {
                    "function_name": "cubic_bezier",
                    "points": [0.35, 0.22, 0.1, 0.8]
                }
            },
        }
    })
}
