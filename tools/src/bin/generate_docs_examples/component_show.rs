use std::{fs, path::Path};

use anyhow::Result;
use serde_json::json;
use tools::compositor_instance::CompositorInstance;

use crate::workingdir;

pub(super) fn generate_show_component_example(root_dir: &Path) -> Result<()> {
    let instance = CompositorInstance::start();
    let mp4_path = root_dir.join("guides/component-show-example.mp4");
    let _ = fs::remove_file(&mp4_path);

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("mp4_example_1.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 0
        }),
    )?;

    instance.send_request(
        "input/input_2/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("mp4_example_2.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 2000
        }),
    )?;

    instance.send_request(
        "input/input_3/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("mp4_example_3.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 5000
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
                "initial": scene(json!([
                    { "type": "input_stream", "input_id": "input_1" }
                ]))
            },
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(json!([
                { "type": "input_stream", "input_id": "input_1" },
                { "type": "input_stream", "input_id": "input_2" }
            ])),
            "schedule_time_ms": 2_000,
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(json!([
                { "type": "input_stream", "input_id": "input_1" },
                { "type": "input_stream", "input_id": "input_2" },
                { "type": "input_stream", "input_id": "input_3" }
            ])),
            "schedule_time_ms": 5_000,
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": scene(json!([
                { "type": "input_stream", "input_id": "input_1" },
                { "type": "input_stream", "input_id": "input_2" },
            ])),
            "schedule_time_ms": 8_000,
        }),
    )?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 12_000,
        }),
    )?;

    instance.send_request("start", json!({}))?;
    instance.wait_for_output_end();

    Ok(())
}

fn scene(children: serde_json::Value) -> serde_json::Value {
    json!({
        "root": {
            "type": "tiles",
            "id": "tiles",
            "transition": { "duration_ms": 300 },
            "background_color": "#52505b",
            "children": children
        }
    })
}
