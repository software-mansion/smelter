use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;

use integration_tests::{
    examples::{self, run_example, TestSample},
    gstreamer::{start_gst_receive_tcp_h264, start_gst_send_tcp},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const INPUT_1_PORT: u16 = 8002;
const INPUT_2_PORT: u16 = 8004;
const OUTPUT_PORT: u16 = 8006;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": INPUT_1_PORT,
            "video": {
                "decoder": "ffmpeg_h264"
            },
        }),
    )?;

    examples::post(
        "input/input_2/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": INPUT_2_PORT,
            "audio": {
                "decoder": "opus"
            },
        }),
    )?;

    let shader_source = include_str!("./silly.wgsl");
    examples::post(
        "shader/shader_example_1/register",
        &json!({
            "source": shader_source,
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": OUTPUT_PORT,
            "audio": {
                "initial": {
                    "inputs": [
                        {"input_id": "input_2"},
                    ]
                },
                "encoder": {
                   "type": "opus",
                   "channels": "stereo",
                }
            },
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
                    "root": {
                        "type": "shader",
                        "id": "shader_node_1",
                        "shader_id": "shader_example_1",
                        "children": [
                            {
                                "id": "input_1",
                                "type": "input_stream",
                                "input_id": "input_1",
                            }
                        ],
                        "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                    }
                }
            }
        }),
    )?;

    start_gst_receive_tcp_h264(IP, OUTPUT_PORT, true)?;

    examples::post("start", &json!({}))?;

    start_gst_send_tcp(
        IP,
        Some(INPUT_1_PORT),
        Some(INPUT_2_PORT),
        TestSample::BigBuckBunny,
    )?;
    Ok(())
}
