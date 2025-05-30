use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::{env, path::PathBuf};

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_receive_h264,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const OUTPUT_PORT: u16 = 8002;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(Some(OUTPUT_PORT), None)?;

    examples::post(
        "image/example_gif/register",
        &json!({
            "url": "https://gifdb.com/images/high/rust-logo-on-fire-o41c0v9om8drr8dv.gif",
        }),
    )?;
    examples::post(
        "image/example_jpeg/register",
        &json!({
            "url": "https://www.rust-lang.org/static/images/rust-social.jpg",
        }),
    )?;
    examples::post(
        "image/example_svg/register",
        &json!({
            "path": PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/assets/rust.svg"),
            "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.width},
        }),
    )?;
    examples::post(
        "image/example_png/register",
        &json!({
            "url": "https://rust-lang.org/logos/rust-logo-512x512.png",
        }),
    )?;

    let new_image = |image_id, label| {
        json!({
            "type": "view",
            "background_color": "#0000FFFF",
            "children": [
                {
                    "type": "rescaler",
                    "mode": "fit",
                    "child": {
                        "type": "image",
                        "image_id": image_id,
                    }
                },
                {
                    "type": "view",
                    "bottom": 20,
                    "right": 20,
                    "width": 400,
                    "height": 40,
                    "children": [{
                        "type": "text",
                        "text": label,
                        "align": "right",
                        "width": 400,
                        "font_size": 40.0,
                        "font_family": "Comic Sans MS",
                    }]
                }
            ]
        })
    };

    let scene = json!({
        "type": "tiles",
        "margin" : 20,
        "children": [
            new_image("example_png", "PNG example"),
            new_image("example_jpeg", "JPEG example"),
            new_image("example_svg", "SVG example"),
            new_image("example_gif", "GIF example"),
        ]
    });

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "port": 8002,
            "ip": "127.0.0.1",
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
                    "root": scene
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    Ok(())
}
