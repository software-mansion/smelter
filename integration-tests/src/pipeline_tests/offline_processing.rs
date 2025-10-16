use std::{
    fs::{self},
    path::Path,
    process::Command,
};

use anyhow::{Result, anyhow};
use regex::Regex;
use serde_json::json;
use smelter::config::read_config;
use tokio_tungstenite::tungstenite;
use tracing::info;

use crate::{CompositorInstance, pipeline_tests::start_server_msg_listener};

const BUNNY_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";

#[test]
pub fn offline_processing() -> Result<()> {
    const OUTPUT_FILE: &str = "/tmp/offline_processing_output.mp4";
    if Path::new(OUTPUT_FILE).exists() {
        fs::remove_file(OUTPUT_FILE)?;
    };

    let mut config = read_config();
    config.ahead_of_time_processing = true;
    config.never_drop_output_frames = true;
    let instance = CompositorInstance::start(Some(config));
    let (msg_sender, msg_receiver) = crossbeam_channel::unbounded();
    start_server_msg_listener(instance.api_port, msg_sender);

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "mp4",
            "url": BUNNY_URL,
            "offset_ms": 0,
            "required": true
        }),
    )?;

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "mp4",
            "path": OUTPUT_FILE,
            "video": {
                "resolution": {
                    "width": 640,
                    "height": 320
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                },
                "initial": {
                    "root": {
                       "type": "view",
                       "children": [{
                            "type": "rescaler",
                            "child": {
                                "type": "input_stream",
                                "input_id": "input_1"
                            }
                        }]
                    }
                },
                "send_eos_when": { "all_inputs": true }
            },
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "aac",
                },
                "initial": {
                    "inputs": [{ "input_id": "input_1" }]
                },
                "send_eos_when": { "all_inputs": true }
            }
        }),
    )?;

    instance.send_request(
        "input/input_1/unregister",
        json!({
            "schedule_time_ms": 2000
        }),
    )?;
    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 2000
        }),
    )?;

    instance.send_request("start", json!({}))?;

    for msg in msg_receiver.iter() {
        if let tungstenite::Message::Text(msg) = msg
            && msg.contains("\"type\":\"OUTPUT_DONE\",\"output_id\":\"output_1\"")
        {
            info!("breaking");
            break;
        }
    }

    let command_output = Command::new("ffprobe")
        .args(["-v", "error", "-show_format", OUTPUT_FILE])
        .output()
        .map_err(|e| anyhow!("Invalid mp4 file. FFprobe error: {}", e))?;

    if !command_output.status.success() {
        return Err(anyhow!(
            "Invalid mp4 file. FFprobe error: {}",
            String::from_utf8_lossy(&command_output.stderr)
        ));
    }

    let output_str = String::from_utf8_lossy(&command_output.stdout);
    let (duration, bit_rate) = extract_ffprobe_info(&output_str)?;

    if !(1.9..=2.1).contains(&duration) {
        return Err(anyhow!("Invalid duration: {}", duration));
    }
    if !(930_000..=1_000_000).contains(&bit_rate) {
        return Err(anyhow!("Invalid bit rate: {}", bit_rate));
    }

    Ok(())
}

fn extract_ffprobe_info(output: &str) -> Result<(f64, u64)> {
    let re_duration = Regex::new(r"duration=(\d+\.\d+)").unwrap();
    let re_bit_rate = Regex::new(r"bit_rate=(\d+)").unwrap();

    let duration: f64 = re_duration
        .captures(output)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().parse().unwrap_or(0.0))
        .ok_or_else(|| anyhow!("Failed to extract duration"))?;

    let bit_rate: u64 = re_bit_rate
        .captures(output)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().parse().unwrap_or(0))
        .ok_or_else(|| anyhow!("Failed to extract bit rate"))?;

    Ok((duration, bit_rate))
}
