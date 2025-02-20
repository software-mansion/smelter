use std::{fs, path::Path};

use anyhow::Result;
use generate::compositor_instance::CompositorInstance;
use serde_json::json;

use crate::workingdir;

pub(super) fn generate_mp4_component_example(root_dir: &Path) -> Result<()> {
    let instance = CompositorInstance::start();
    let mp4_path = root_dir.join("guides/component-mp4-example.mp4");
    let _ = fs::remove_file(&mp4_path);

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("mp4_1.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 0
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
            "schedule_time_ms": 10_000,
        }),
    )?;

    instance.send_request("start", json!({}))?;
    instance.wait_for_output_end();

    Ok(())
}

fn scene() -> serde_json::Value {
    json!({
        "root": {
            "type": "rescaler",
            "child": {
                "type": "input_stream",
                "input_id": "input_1",
            }
        }
    })
}
