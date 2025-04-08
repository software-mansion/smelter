use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;
use std::{process::Command, thread::sleep, time::Duration};

use integration_tests::{
    examples::{self, run_example},
    gstreamer::start_gst_receive_tcp_vp8,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const INPUT_PORT: u16 = 8002;
const OUTPUT_PORT: u16 = 8004;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_PORT,
            "video": {
                "decoder": "ffmpeg_vp8"
            }
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "port": OUTPUT_PORT,
            "transport_protocol": "tcp_server",
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_vp8",
                },
                "initial": {
                    "root": {
                        "type": "view",
                        "background_color": "#4d4d4dff",
                        "children": [
                            {
                              "type": "rescaler",
                              "width": VIDEO_RESOLUTION.width,
                              "height": VIDEO_RESOLUTION.height,
                              "child": {
                                "type": "input_stream",
                                "input_id": "input_1"
                              }
                            }
                        ]
                    }
                },
            },
        }),
    )?;

    start_gst_receive_tcp_vp8(IP, OUTPUT_PORT, false)?;
    examples::post("start", &json!({}))?;

    let gst_input_command = format!("ffmpeg -f lavfi -i testsrc=size=1280x720:rate=30 -c:v libvpx -format yuv420p -f rtp rtp://{IP}:{INPUT_PORT}");
    Command::new("bash")
        .arg("-c")
        .arg(gst_input_command)
        .spawn()?;
    sleep(Duration::from_secs(300));
    examples::post("output/output_1/unregister", &json!({}))?;

    Ok(())
}
