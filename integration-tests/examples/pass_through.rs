use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;

use integration_tests::{
    examples::{self, run_example},
    media::{MediaReceiver, MediaSender, Receive, Send, TestSample},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const INPUT_PORT: u16 = 8002;
const OUTPUT_PORT: u16 = 8004;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    MediaReceiver::new(Receive::rtmp_listener(OUTPUT_PORT)).spawn()?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_PORT,
            "video": {
                "decoder": "ffmpeg_h264"
            }
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtmp_client",
            "url": format!("rtmp://127.0.0.1:{OUTPUT_PORT}"),
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
                        "id": "input_1",
                        "type": "input_stream",
                        "input_id": "input_1",
                    }
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    MediaSender::new(
        TestSample::OceanSampleH264,
        Send::rtp_udp_client().video_port(INPUT_PORT),
    )
    .with_looped_input(true)
    .spawn()?;
    Ok(())
}
