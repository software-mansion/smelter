use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use generate::compositor_instance::CompositorInstance;
use serde_json::json;

use crate::workingdir;

pub(super) fn generate_view_transition_guide(root_dir: &Path) -> Result<()> {
    generate_scene(
        root_dir.join("guides/transition-width.mp4"),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 480,
                    "child": { "type": "input_stream", "input_id": "input_1" },
                }
            ]
        }),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 1280,
                    "transition": {
                        "duration_ms": 2000,
                    },
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
            ]
        }),
    )?;

    generate_scene(
        root_dir.join("guides/transition-sibling-width.mp4"),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 480,
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "type": "rescaler",
                    "child": { "type": "input_stream", "input_id": "input_2" },
                }

            ]
        }),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 1280,
                    "transition": {
                        "duration_ms": 2000,
                    },
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "type": "rescaler",
                    "child": { "type": "input_stream", "input_id": "input_2" },
                }
            ]
        }),
    )?;

    // generate_scene(
    //     json!({
    //         "type": "view",
    //         "background_color": "#52505bff",
    //         "children": [
    //             {
    //                 "id": "rescaler_1",
    //                 "type": "rescaler",
    //                 "width": 480,
    //                 "child": { "type": "input_stream", "input_id": "input_1" },
    //             },
    //         ]
    //     }),
    //     json!({
    //         "type": "view",
    //         "background_color": "#52505bff",
    //         "children": [
    //             {
    //                 "id": "rescaler_1",
    //                 "type": "rescaler",
    //                 "width": 1280,
    //                 "top": 0,
    //                 "left": 0,
    //                 "transition": { "duration_ms": 2000 },
    //                 "child": { "type": "input_stream", "input_id": "input_1" },
    //             },
    //         ]
    //     }),
    // )?;

    generate_scene(
        root_dir.join("guides/transition-interpolation-functions.mp4"),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 0, "left": 0,
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "id": "rescaler_2",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 0, "left": 320,
                    "child": { "type": "input_stream", "input_id": "input_2" },
                },
                {
                    "id": "rescaler_3",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 0, "left": 640,
                    "child": { "type": "input_stream", "input_id": "input_3" },
                },
                {
                    "id": "rescaler_4",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 0, "left": 960,
                    "child": { "type": "input_stream", "input_id": "input_4" },
                },
            ]
        }),
        json!({
            "type": "view",
            "background_color": "#52505bff",
            "children": [
                {
                    "id": "rescaler_1",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 540, "left": 0,
                    "transition": { "duration_ms": 2000 },
                    "child": { "type": "input_stream", "input_id": "input_1" },
                },
                {
                    "id": "rescaler_2",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 540, "left": 320,
                    "transition": { "duration_ms": 2000, "easing_function": {"function_name": "bounce"} },
                    "child": { "type": "input_stream", "input_id": "input_2" },
                },
                {
                    "id": "rescaler_3",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 540, "left": 640,
                    "child": { "type": "input_stream", "input_id": "input_3" },
                    "transition": {
                        "duration_ms": 2000,
                        "easing_function": {
                            "function_name": "cubic_bezier",
                            "points": [0.65, 0, 0.35, 1]
                        }
                    },
                },
                {
                    "id": "rescaler_4",
                    "type": "rescaler",
                    "width": 320, "height": 180, "top": 540, "left": 960,
                    "child": { "type": "input_stream", "input_id": "input_4" },
                    "transition": {
                        "duration_ms": 2000,
                        "easing_function": {
                            "function_name": "cubic_bezier",
                            "points": [0.33, 1, 0.68, 1]
                        }
                    },
                },
            ]
        }),
    )?;
    Ok(())
}

pub(super) fn generate_scene(
    mp4_path: PathBuf,
    scene_start: serde_json::Value,
    scene_change: serde_json::Value,
) -> Result<()> {
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
                    "root": scene_start,
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
        "input/input_3/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("input_3.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 0
        }),
    )?;

    instance.send_request(
        "input/input_4/register",
        json!({
            "type": "mp4",
            "path": workingdir().join("input_4.mp4").to_str().unwrap(),
            "required": true,
            "offset_ms": 0
        }),
    )?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 8_000,
        }),
    )?;

    instance.send_request(
        "output/output_1/update",
        json!({
            "video": {
                "root": scene_change
            },
            "schedule_time_ms": 2000
        }),
    )?;

    instance.send_request("start", json!({}))?;
    instance.wait_for_output_end();

    Ok(())
}
