use anyhow::{anyhow, Result};
use log::{error, info, warn};
use serde_json::json;
use signal_hook::{consts, iterator::Signals};
use smelter::config::read_config;
use smelter_api::Resolution;
use std::{env, process::Command, thread, time::Duration};

use integration_tests::{
    examples::{self, start_server_msg_listener, TestSample},
    ffmpeg::{start_ffmpeg_receive_h264, start_ffmpeg_send},
    paths::integration_tests_root,
};
const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const IP: &str = "127.0.0.1";
const INPUT_PORT: u16 = 8002;
const OUTPUT_PORT: u16 = 8004;

fn main() {
    let Ok(host_ip) = env::var("DOCKER_HOST_IP") else {
        if cfg!(target_os = "macos") {
            error!(
                "DOCKER_HOST_IP is not specified. You can find ip using 'ipconfig getifaddr en0' or 'ipconfig getifaddr en1'."
            );
        } else {
            error!(
                "DOCKER_HOST_IP is not specified. You can find ip using 'ip addr show docker0'."
            );
        }
        return;
    };

    let skip_build = env::var("SKIP_DOCKER_REBUILD").is_ok();

    build_and_start_docker(skip_build).unwrap();

    thread::spawn(|| {
        if let Err(err) = start_example_client_code(host_ip) {
            error!("{err}")
        }
    });

    let mut signals = Signals::new([consts::SIGINT]).unwrap();
    signals.forever().next();
}

fn build_and_start_docker(skip_build: bool) -> Result<()> {
    if !skip_build {
        info!("[example] docker build");
        let mut process = Command::new("docker")
            .args([
                "build",
                "-f",
                integration_tests_root()
                    .parent()
                    .unwrap()
                    .join("build_tools/docker/slim.Dockerfile")
                    .to_str()
                    .unwrap(),
                "-t",
                "video-compositor",
                ".",
            ])
            .current_dir(integration_tests_root().parent().unwrap())
            .spawn()?;
        let exit_code = process.wait()?;
        if Some(0) != exit_code.code() {
            return Err(anyhow!("Docker build finished with exit code {exit_code}"));
        }
    } else {
        warn!("Skipping image build, using old version.")
    }

    let mut args = vec![
        "run",
        "-it",
        "-p",
        format!("{INPUT_PORT}:{INPUT_PORT}/udp").leak(),
        "-p",
        format!("{}:{}", read_config().api_port, read_config().api_port).leak(),
        "--rm",
    ];

    if env::var("NVIDIA").is_ok() {
        info!("[example] configured for nvidia GPUs");
        args.extend_from_slice(&["--gpus", "all", "--runtime=nvidia"]);
    } else if env::var("NO_GPU").is_ok() || cfg!(target_os = "macos") {
        info!("[example] configured for software based rendering");
    } else {
        info!("[example] configured for non-nvidia GPUs");
        args.extend_from_slice(&["--device", "/dev/dri"]);
    }

    args.push("video-compositor");

    info!("[example] docker run");
    Command::new("docker").args(args).spawn()?;
    Ok(())
}

fn start_example_client_code(host_ip: String) -> Result<()> {
    thread::sleep(Duration::from_secs(5));

    start_ffmpeg_receive_h264(Some(OUTPUT_PORT), None)?;
    start_server_msg_listener();

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

    let shader_source = include_str!("./silly.wgsl");
    examples::post(
        "shader/example_shader/register",
        &json!({
            "source": shader_source,
        }),
    )?;

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "port": OUTPUT_PORT,
            "ip": host_ip,
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
                        "type": "shader",
                        "shader_id": "example_shader",
                        "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                        "children": [
                            {
                               "type": "input_stream",
                               "input_id": "input_1",
                            }
                        ]
                    }
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    start_ffmpeg_send(IP, Some(INPUT_PORT), None, TestSample::TestPatternH264)?;

    Ok(())
}
