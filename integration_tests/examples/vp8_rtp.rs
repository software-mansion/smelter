use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;
use std::{
    process::{Command, Stdio},
    thread::sleep,
    time::Duration,
};

use integration_tests::examples::{self, run_example};

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
        "input/input_2/register",
        &json!({
            "type": "rtp_stream",
            "port": 8008,
            "audio": {
                "decoder": "opus",
                "forward_error_correction": true,
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
            "audio": {
                "encoder": {
                    "type": "opus",
                    "channels": "stereo",
                    "forward_error_correction": true,
                },
                "initial": {
                    "inputs": [
                        {"input_id": "input_2"}
                    ]
                }
            }
        }),
    )?;

    //TODO only temporary
    let mut gst_output_command = [
        "gst-launch-1.0 -v ",
        "rtpptdemux name=demux ",
        &format!("tcpclientsrc host={} port={} ! \"application/x-rtp-stream\" ! rtpstreamdepay ! queue ! demux. ", IP, OUTPUT_PORT)
        ].concat();
    gst_output_command.push_str("demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=VP8\" ! queue ! rtpvp8depay ! decodebin ! videoconvert ! autovideosink ");
    gst_output_command.push_str("demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink ");

    Command::new("bash")
        .arg("-c")
        .arg(gst_output_command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    sleep(Duration::from_secs(2));

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
