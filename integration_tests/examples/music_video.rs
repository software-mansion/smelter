use anyhow::Result;
use compositor_api::Resolution;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::info;

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_receive_h264,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

const IP: &str = "127.0.0.1";
const INPUT_VIDEO_PORT: u16 = 8002;
const INPUT_AUDIO_PORT: u16 = 8004;

const OUTPUT_VIDEO_PORT: u16 = 8006;
const OUTPUT_AUDIO_PORT: u16 = 8008;

const PATH: &str = env!("CARGO_MANIFEST_DIR");

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

fn ffmpeg_audio_stream_from_file(path: &PathBuf, ip: &str, port: u16) -> Result<()> {
    info!("[example] Start sending audio loop to input port {}.", port);
    Command::new("ffmpeg")
        .args(["-stream_loop", "-1", "-re", "-i"])
        .arg(path)
        .args([
            "-vn",
            "-c:a",
            "copy",
            // "libopus",
            // "-ar",
            // "48000",
            "-f",
            "rtp",
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
    start_ffmpeg_receive_h264(Some(OUTPUT_VIDEO_PORT), Some(OUTPUT_AUDIO_PORT))?;
    start_ffmpeg_receive_h264(None, Some(OUTPUT_AUDIO_PORT))?;

    examples::post(
        "input/input_video/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_VIDEO_PORT,
            "video": {
                "decoder": "ffmpeg_h264"
            }
        }),
    )?;

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
        "output/output_video/register",
        &json!({
            "type": "rtp_stream",
            "port": OUTPUT_VIDEO_PORT,
            "ip": IP,
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
                        "type": "input_stream",
                        "input_id": "input_video",
                    }
                }
            }
        }),
    )?;

    examples::post(
        "output/output_audio/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_AUDIO_PORT,
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "opus",
                    "sample_rate": 48000,
                    "preset": "quality",
                },
                "initial": {
                    "inputs": [
                        {
                            "input_id": "input_audio",
                        },
                    ],
                },
            },
        }),
    )?;

    let path = Path::new(PATH).join("examples/assets/lachrymaSkipIntroOpusAudio.mp4");
    // let path = Path::new(PATH).join("examples/assets/lachrymaSkipIntro.mp4");

    ffmpeg_video_stream_from_file(&path, IP, INPUT_VIDEO_PORT)?;
    ffmpeg_audio_stream_from_file(&path, IP, INPUT_AUDIO_PORT)?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    Ok(())
}
