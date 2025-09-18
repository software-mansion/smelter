use std::{fs, path::PathBuf, time::Duration};

use anyhow::Result;
use serde_json::json;
use tools::compositor_instance::CompositorInstance;

fn main() {
    let _ = fs::remove_dir_all(workingdir());
    fs::create_dir_all(workingdir()).unwrap();

    // HSV 255°, 56%, 67% (navy blue)
    generate_video(workingdir().join("input_1.mp4"), "Input 1", "#624baaff").unwrap();
    // HSV 350°, 71%, 75% (red)
    generate_video(workingdir().join("input_2.mp4"), "Input 2", "#bf374eff").unwrap();
    // HSV 142°, 63%, 64% (green)
    generate_video(workingdir().join("input_3.mp4"), "Input 3", "#3da362ff").unwrap();
    // HSV 60°, 50%, 65% (yellow)
    generate_video(workingdir().join("input_4.mp4"), "Input 4", "#a6a653ff").unwrap();
    // HSV 180°, 50%, 65% (light blue)
    generate_video(workingdir().join("input_5.mp4"), "Input 5", "#53a6a6ff").unwrap();
    // HSV 300°, 50%, 65% (purple)
    generate_video(workingdir().join("input_6.mp4"), "Input 6", "#a653a6ff").unwrap();

    generate_video(workingdir().join("mp4_1.mp4"), "Example MP4", "#624baaff").unwrap();

    generate_video(
        workingdir().join("mp4_example_1.mp4"),
        "Example MP4 - 1",
        "#624baaff",
    )
    .unwrap();
    generate_video(
        workingdir().join("mp4_example_2.mp4"),
        "Example MP4 - 2",
        "#bf374eff",
    )
    .unwrap();
    generate_video(
        workingdir().join("mp4_example_3.mp4"),
        "Example MP4 - 3",
        "#3da362ff",
    )
    .unwrap();
}

fn workingdir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("workingdir")
        .join("inputs")
}

fn generate_video(path: PathBuf, text: &str, rgba_color: &str) -> Result<()> {
    let instance = CompositorInstance::start();

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "mp4",
            "path": path.to_str().unwrap(),
            "video": {
                "resolution": {
                    "width": 1920,
                    "height": 1080,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                },
                "initial": scene(text, rgba_color, Duration::ZERO)
            },
        }),
    )?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20_000,
        }),
    )?;

    const EVENT_COUNT: u64 = 2_000;
    for i in 0..EVENT_COUNT {
        let pts = Duration::from_millis(20_000 * i / EVENT_COUNT);
        instance.send_request(
            "output/output_1/update",
            json!({
                "video": scene(text, rgba_color, pts),
                "schedule_time_ms": pts.as_millis(),
            }),
        )?;
    }

    instance.send_request("start", json!({}))?;
    instance.wait_for_output_end();

    Ok(())
}

fn scene(text: &str, rgba_color: &str, pts: Duration) -> serde_json::Value {
    json!({
        "root": {
            "type": "view",
            "background_color": rgba_color,
            "direction": "column",
            "children": [
                { "type": "view" },
                {
                    "type": "text",
                    "text": text,
                    "font_size": 230,
                    "width": 1920,
                    "align": "center",
                    "font_family": "Comic Sans MS",
                },
                { "type": "view" },
                {
                  "type": "view",
                  "bottom": 100,
                  "right": 100,
                  "width":  300,
                  "height": 100,
                  "children": [
                     {
                            "type": "text",
                            "text": format!("{:.2}s", pts.as_millis() as f32 / 1000.0),
                            "font_size": 90,
                            "width": 300,
                            "align": "right",
                            "font_family": "Comic Sans MS",
                     },
                  ]
                }
            ]
        }
    })
}
