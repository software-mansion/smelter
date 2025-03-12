use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;
use std::{process::Command, thread::sleep, time::Duration};
use tracing::info;

use integration_tests::{
    examples::{self, run_example},
    gstreamer::start_gst_receive_tcp,
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

    let token_input_1 = examples::post(
        "input/input_2/register",
        &json!({
            "type": "whip",
        }),
    )?
    .json::<serde_json::Value>();

    if let Ok(token) = token_input_1 {
        info!("Bearer token for input_2: {}", token["bearer_token"]);
    }

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
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast"
                },
                "initial": {
                    "root": {
                        "type": "view",
                        "background_color": "#4d4d4dff",
                        "children": [
                            {
                                "type": "rescaler",
                                    "child": {
                                    "type": "input_stream",
                                    "input_id": "input_1"
                                }
                            },
                            {
                                "type": "rescaler",
                                "child": {
                                    "type": "input_stream",
                                    "input_id": "input_2"
                                }
                            }
                        ]
                    }
                },

            },
            "audio": {
                "encoder": {
                    "type": "opus",
                    "channels": "stereo",
                },
                "initial": {
                    "inputs": [
                        {"input_id": "input_2"}
                    ]
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));
    start_gst_receive_tcp(IP, OUTPUT_PORT, true, true)?;
    examples::post("start", &json!({}))?;

    let gst_input_command = format!("gst-launch-1.0 videotestsrc pattern=ball ! video/x-raw,width=1280,height=720 ! vp8enc ! rtpvp8pay ! udpsink host=127.0.0.1 port={INPUT_PORT}");
    Command::new("bash")
        .arg("-c")
        .arg(gst_input_command)
        .spawn()?;
    sleep(Duration::from_secs(300));
    examples::post("output/output_1/unregister", &json!({}))?;

    Ok(())
}
