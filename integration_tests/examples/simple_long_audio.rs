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
const INPUT_AUDIO_PORT: u16 = 8004;

const OUTPUT_AUDIO_PORT: u16 = 8008;

const PATH: &str = env!("CARGO_MANIFEST_DIR");

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
    start_ffmpeg_receive_h264(None, Some(OUTPUT_AUDIO_PORT))?;

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

    let path = Path::new(PATH).join("examples/assets/lachrymaAudioOnly2PercentLoss.opus");
    // let path = Path::new(PATH).join("examples/assets/lachrymaAudioOnly10PercentLoss.opus");
    // let path = Path::new(PATH).join("examples/assets/lachrymaAudioOnly20PercentLoss.opus");
    // let path = Path::new(PATH).join("examples/assets/lachrymaSkipIntro.mp4");

    ffmpeg_audio_stream_from_file(&path, IP, INPUT_AUDIO_PORT)?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    Ok(())
}
