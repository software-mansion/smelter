use anyhow::Result;
use serde_json::json;
use smelter_api::Resolution;
use std::time::Duration;

use integration_tests::{
    examples::{self, run_example},
    media::{MediaReceiver, MediaSender, Receive, Send, TestSample, VideoCodec},
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const INPUT_1_PORT: u16 = 8002;
const INPUT_2_PORT: u16 = 8004;
const OUTPUT_VIDEO_PORT: u16 = 8010;
const OUTPUT_AUDIO_PORT: u16 = 8012;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    MediaReceiver::new(
        Receive::rtp_udp_listener()
            .video(OUTPUT_VIDEO_PORT, VideoCodec::H264)
            .audio_port(OUTPUT_AUDIO_PORT),
    )
    .spawn()?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
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
            "port": INPUT_2_PORT,
            "audio": {
                "decoder": "aac",
                // both of these options can be acquired by passing the
                // `-sdp_file FILENAME` flag to the ffmpeg instance which will
                // stream data to the compositor.
                // ffmpeg will then write out an sdp file containing both fields.
                "rtp_mode": "high_bitrate",
                "audio_specific_config": "121056E500",
            },
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_VIDEO_PORT,
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "fast"
                },
                "initial": {
                    "root": {
                        "type": "input_stream",
                        "input_id": "input_1"
                    }
                },
                "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
            }
        }),
    )?;

    examples::post(
        "output/output_2/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_AUDIO_PORT,
            "audio": {
                "initial": {
                    "inputs": [
                        {"input_id": "input_2"},
                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                }
            }
        }),
    )?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    MediaSender::new(
        TestSample::BigBuckBunnyH264AAC,
        Send::rtp_udp_client()
            .video_port(INPUT_1_PORT)
            .audio_port(INPUT_2_PORT),
    )
    .spawn()?;

    Ok(())
}
