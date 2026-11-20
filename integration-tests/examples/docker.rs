use anyhow::{Result, anyhow};
use serde_json::json;
use signal_hook::{consts, iterator::Signals};
use smelter::{config::read_config, logger::init_logger};
use smelter_api::Resolution;
use std::{
    env,
    process::{self, Command},
    thread,
    time::Duration,
};
use tracing::{error, info, warn};

use integration_tests::{
    examples::{self, start_server_msg_listener, wait_for_server_ready},
    media::{MediaSender, Send, TestSample},
    paths::integration_tests_root,
};
const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const STREAM_KEY: &str = "example";
const BEARER_TOKEN: &str = "example";

const CONTAINER_NAME: &str = "smelter-example";

const HTML_FILE_PATH: &str = "examples/web_view.html";
const CONTAINER_HTML_FILE_PATH: &str = "/web_view.html";

fn main() {
    init_logger(read_config().logger);

    let skip_build = env::var("SKIP_DOCKER_REBUILD").is_ok();

    build_and_start_docker(skip_build).unwrap();

    thread::spawn(|| {
        if let Err(err) = start_example_client_code() {
            error!("{err}");
            stop_docker();
            process::exit(1);
        }
    });

    let mut signals = Signals::new([consts::SIGINT, consts::SIGTERM]).unwrap();
    signals.forever().next();
    stop_docker();
}

fn stop_docker() {
    info!("[example] docker kill");
    if let Err(err) = Command::new("docker")
        .args(["kill", CONTAINER_NAME])
        .status()
    {
        error!("Failed to stop the container: {err}");
    }
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
                    .join("tools/docker/full.Dockerfile")
                    .to_str()
                    .unwrap(),
                "-t",
                "smelter-example",
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

    let config = read_config();
    let mut args = vec![
        "run",
        "-p",
        format!("{0}:{0}", config.api_port).leak(),
        "-p",
        format!("{0}:{0}", config.rtmp_server_port).leak(),
        "-p",
        format!("{0}:{0}", config.whip_whep_server_port).leak(),
        "--rm",
        "--name",
        CONTAINER_NAME,
        // Container is stopped explicitly on exit.
        "--sig-proxy=false",
        "-e",
        "SMELTER_WEB_RENDERER_ENABLE=1",
        "-v",
        format!(
            "{}:{CONTAINER_HTML_FILE_PATH}:ro",
            integration_tests_root().join(HTML_FILE_PATH).display()
        )
        .leak(),
    ];

    if env::var("NVIDIA").is_ok() {
        info!("[example] configured for nvidia GPUs");
        args.extend_from_slice(&["--gpus", "all", "--runtime=nvidia"]);
    } else if env::var("NO_GPU").is_ok() || cfg!(target_os = "macos") {
        info!("[example] configured for software based rendering");
    } else {
        info!("[example] configured for non-nvidia GPUs");
        args.extend_from_slice(&[
            "--device",
            "/dev/dri",
            "-e",
            "RADV_PERFTEST=video_decode,video_encode",
        ]);
    }

    args.push("smelter-example");

    info!("[example] docker run");
    Command::new("docker").args(args).spawn()?;
    Ok(())
}

fn start_example_client_code() -> Result<()> {
    wait_for_server_ready(Duration::from_secs(30))?;

    let config = read_config();
    start_server_msg_listener();

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtmp_server",
            "stream_key": STREAM_KEY,
        }),
    )?;

    let shader_source = include_str!("./silly.wgsl");
    examples::post(
        "shader/example_shader/register",
        &json!({
            "source": shader_source,
        }),
    )?;

    let mut tiles = vec![json!({
        "type": "shader",
        "shader_id": "example_shader",
        "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
        "children": [
            {
               "type": "input_stream",
               "input_id": "input_1",
            }
        ]
    })];

    let web_renderer_result = examples::post(
        "web-renderer/example_website/register",
        &json!({
            "url": format!("file://{CONTAINER_HTML_FILE_PATH}"),
            "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
        }),
    );
    match web_renderer_result {
        Ok(_) => tiles.push(json!({
            "type": "web_view",
            "instance_id": "example_website",
            "children": [
                {
                    "id": "big_bunny_video",
                    "type": "input_stream",
                    "input_id": "input_1",
                }
            ]
        })),
        Err(_) => warn!("[example] Web renderer registration failed, rendering without web view."),
    }

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "whep_server",
            "bearer_token": BEARER_TOKEN,
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "ffmpeg_options": { "preset": "ultrafast" }
                },
                "initial": {
                    "root": {
                        "type": "tiles",
                        "children": tiles,
                    }
                }
            },
            "audio": {
                "encoder": {
                    "type": "opus"
                },
                "initial": {
                    "inputs": [{ "input_id": "input_1" }]
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    MediaSender::new(
        TestSample::BigBuckBunnyH264AAC,
        Send::rtmp_client(format!(
            "rtmp://127.0.0.1:{}/input_1/{STREAM_KEY}",
            config.rtmp_server_port
        )),
    )
    .spawn()?;

    info!(
        "[example] Open in browser to receive stream: \
        https://smelter-labs.github.io/tools/#whep-player?url=http://127.0.0.1:{}/whep/output_1&token={BEARER_TOKEN}",
        config.whip_whep_server_port
    );

    Ok(())
}
