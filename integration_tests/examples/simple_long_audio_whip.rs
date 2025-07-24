use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::info;

use integration_tests::examples::{self, run_example};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const INPUT_AUDIO_PORT: u16 = 8004;
const INPUT_VIDEO_PORT: u16 = 8006;

const PATH: &str = env!("CARGO_MANIFEST_DIR");

fn ffmpeg_audio_stream_from_file(path: &PathBuf, ip: &str, port: u16) -> Result<()> {
    info!("[example] Start sending audio loop to input port {}.", port);
    Command::new("ffmpeg")
        .args(["-stream_loop", "-1", "-re", "-i"])
        .arg(path)
        .args([
            "-vn",
            "-c:a",
            // "copy",
            "libopus",
            "-ar",
            "48000",
            "-f",
            "rtp",
            &format!("rtp://{}:{}?rtcpport={}", ip, port, port),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

fn ffmpeg_video_stream_from_file(path: &PathBuf, ip: &str, port: u16) -> Result<()> {
    info!("[example] Start sending video loop to input port {}.", port);
    Command::new("ffmpeg")
        .args(["-stream_loop", "-1", "-re", "-i"])
        .arg(path)
        .args([
            "-an",
            "-c:v",
            "copy",
            "-f",
            "rtp",
            "-bsf:v",
            "h264_mp4toannexb",
            &format!("rtp://{}:{}?rtcpport={}", ip, port, port),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    examples::post(
        "input/input_audio/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_AUDIO_PORT,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_video/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_VIDEO_PORT,
            "video": {
                "decoder": "ffmpeg_h264",
            },
        }),
    )?;

    examples::post(
        "output/output/register",
        &json!({
            "type": "whip",
            "endpoint_url": "http://127.0.0.1:8080/api/whip",
            "bearer_token": "example",
            "audio": {
                "channels": "stereo",
                "mixing_strategy": "sum_scale",
                "encoder_preferences": [
                    {
                        "type": "opus",
                        "sample_rate": 48000,
                        "preset": "quality",
                        "forward_error_correction": true,
                    },
                    {
                        "type": "any",
                    },
                ],
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_audio",
                        },
                    ],
                },
            },
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "initial": {
                    "root": {
                        "type": "input_stream",
                        "input_id": "input_video",
                    },
                },
            },
        }),
    )?;

    // let path = Path::new(PATH).join("examples/assets/lachrymaAudioOnly2PercentLoss.opus");
    // let path = Path::new(PATH).join("examples/assets/lachrymaAudioOnly10PercentLoss.opus");
    // let path = Path::new(PATH).join("examples/assets/lachrymaAudioOnly20PercentLoss.opus");
    let path = Path::new(PATH).join("examples/assets/lachrymaSkipIntro.mp4");

    ffmpeg_audio_stream_from_file(&path, IP, INPUT_AUDIO_PORT)?;
    ffmpeg_video_stream_from_file(&path, IP, INPUT_VIDEO_PORT)?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    Ok(())
}
