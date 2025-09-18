use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;
use std::{thread::sleep, time::Duration};

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_send,
    gstreamer::start_gst_receive_tcp_vp8,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
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

    start_ffmpeg_send(
        IP,
        Some(INPUT_PORT),
        None,
        examples::TestSample::ElephantsDreamVP8Opus,
    )?;

    sleep(Duration::from_secs(300));
    examples::post("output/output_1/unregister", &json!({}))?;

    Ok(())
}
