use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;
use std::time::Duration;

use integration_tests::{
    examples::{self, run_example, TestSample},
    ffmpeg::{start_ffmpeg_receive_vp8, start_ffmpeg_send},
    gstreamer::start_gst_send_udp,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const INPUT_1_PORT: u16 = 8002;
const INPUT_2_PORT: u16 = 8004;
const INPUT_3_PORT: u16 = 8006;
const INPUT_4_PORT: u16 = 8008;
const INPUT_5_PORT: u16 = 8010;
const INPUT_6_PORT: u16 = 8012;
const INPUT_7_PORT: u16 = 8014;
const OUTPUT_VIDEO_PORT: u16 = 8016;
const OUTPUT_AUDIO_PORT: u16 = 8018;

fn main() {
    run_example(start_example_client_code);
}

fn start_example_client_code() -> Result<()> {
    start_ffmpeg_receive_vp8(Some(OUTPUT_VIDEO_PORT), Some(OUTPUT_AUDIO_PORT))?;

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
                "decoder": "opus"
            },
        }),
    )?;

    examples::post(
        "input/input_3/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_3_PORT,
            "video": {
                "decoder": "ffmpeg_vp8"
            },
        }),
    )?;

    examples::post(
        "input/input_4/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_4_PORT,
            "audio": {
                "decoder": "opus"
            },
        }),
    )?;

    examples::post(
        "input/input_5/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_5_PORT,
            "video": {
                "decoder": "ffmpeg_vp8"
            },
        }),
    )?;

    examples::post(
        "input/input_6/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_6_PORT,
            "video": {
                "decoder": "ffmpeg_h264"
            },
        }),
    )?;

    examples::post(
        "input/input_7/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_7_PORT,
            "video": {
                "decoder": "ffmpeg_vp8"
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
                    "type": "ffmpeg_vp8",
                },
                "initial": {
                    "root": {
                        "type": "tiles",
                        "children": [
                            {
                                "type": "input_stream",
                                "input_id": "input_1"
                            },
                            {
                                "type": "input_stream",
                                "input_id": "input_3"
                            },
                            {
                                "type": "input_stream",
                                "input_id": "input_5"
                            },
                            {
                                "type": "input_stream",
                                "input_id": "input_6"
                            },
                            {
                                "type": "input_stream",
                                "input_id": "input_7"
                            }
                        ]
                    }
                },
                "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
            },
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
                        {"input_id": "input_4"}
                    ]
                },
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                }
            },
        }),
    )?;
    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    start_gst_send_udp(
        IP,
        Some(INPUT_1_PORT),
        Some(INPUT_2_PORT),
        TestSample::BigBuckBunnyH264Opus,
    )?;
    start_ffmpeg_send(
        IP,
        Some(INPUT_3_PORT),
        Some(INPUT_4_PORT),
        TestSample::ElephantsDreamVP8Opus,
    )?;
    start_gst_send_udp(IP, Some(INPUT_5_PORT), None, TestSample::SampleVP8)?;
    start_ffmpeg_send(IP, Some(INPUT_6_PORT), None, TestSample::SampleLoopH264)?;
    start_ffmpeg_send(IP, Some(INPUT_7_PORT), None, TestSample::TestPatternVP8)?;

    Ok(())
}
