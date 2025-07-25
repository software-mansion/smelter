use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_rtmp_receive,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const OUTPUT_PORT: u16 = 9002;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() != 2 {
        println!("Usage: {} <HLS playlist url>", args[0]);
        return Ok(());
    }

    start_ffmpeg_rtmp_receive(OUTPUT_PORT)?;

    examples::post("start", &json!({}))?;
    examples::post(
        "input/input_1/register",
        &json!({
            "type": "hls",
            "url": args[1],
            "decoder_map": {
                "h264": "ffmpeg_h264"
            }
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "url": format!("rtmp://{IP}:{OUTPUT_PORT}"),
            "type": "rtmp_client",
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                    "ffmpeg_options": {
                        "g": "120", // keyframe every 100 frames
                        "b": "6M"   // bitrate 6000 kb/s
                    }
                },
                "initial": {
                    "root": {
                        "type": "input_stream",
                        "input_id": "input_1"
                    }
                }
            },
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "aac",
                    "sample_rate": 44100
                },
                "initial": {
                    "inputs": [
                        {"input_id": "input_1"}
                    ]
                }
            }
        }),
    )?;

    Ok(())
}
