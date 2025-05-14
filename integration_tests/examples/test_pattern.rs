use anyhow::Result;
use compositor_api::Resolution;
use serde::Deserialize;
use serde_json::json;

use integration_tests::{
    examples::{self, run_example, TestSample},
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_send},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const IP: &str = "127.0.0.1";
const OUTPUT_PORT: u16 = 8002;

fn main() {
    run_example(client_code);
}

#[derive(Deserialize)]
struct RegisterResponse {
    port: u16,
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(Some(OUTPUT_PORT), None)?;

    let RegisterResponse { port: input_port } = examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "port": "8004:8008",
            "video": {
                "decoder": "ffmpeg_h264"
            },
            "offset_ms": 0,
            "required": true,
        }),
    )?
    .json::<RegisterResponse>()?;

    let shader_source = include_str!("./silly.wgsl");
    examples::post(
        "shader/example_shader/register",
        &json!({
            "source": shader_source,
        }),
    )?;

    let scene = json!( {
        "type": "shader",
        "id": "shader_1",
        "shader_id": "example_shader",
        "children": [
            {
                "type": "input_stream",
                "input_id": "input_1",
            }
        ],
        "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
    });

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "port": OUTPUT_PORT,
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

    start_ffmpeg_send(IP, Some(input_port), None, TestSample::TestPatternH264)?;

    examples::post("start", &json!({}))?;

    Ok(())
}
