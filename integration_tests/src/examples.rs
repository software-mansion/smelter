use anyhow::{anyhow, Result};

use futures_util::{SinkExt, StreamExt};
use reqwest::{blocking::Response, StatusCode};
use smelter::{config::read_config, server};
use std::{
    env,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    process::{self, Command},
    thread,
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite;
use tracing::{error, info, warn};

use serde::Serialize;

pub fn post<T: Serialize + ?Sized>(route: &str, json: &T) -> Result<Response> {
    info!("[example] Sent post request to `{route}`.");

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!(
            "http://127.0.0.1:{}/api/{}",
            read_config().api_port,
            route
        ))
        .timeout(Duration::from_secs(100))
        .json(json)
        .send()
        .unwrap();
    if response.status() >= StatusCode::BAD_REQUEST {
        log_request_error(&json, response);
        return Err(anyhow!("Request failed."));
    }
    Ok(response)
}

pub fn run_example(client_code: fn() -> Result<()>) {
    thread::spawn(move || {
        ffmpeg_next::format::network::init();

        download_all_assets().unwrap();

        if let Err(err) = wait_for_server_ready(Duration::from_secs(10)) {
            error!("{err}");
            process::exit(1);
        }

        thread::spawn(move || {
            if let Err(err) = client_code() {
                error!("{err}");
                process::exit(1);
            }
        });

        start_server_msg_listener();
    });
    server::run();
}

fn wait_for_server_ready(timeout: Duration) -> Result<()> {
    let server_status_url = "http://127.0.0.1:8081/status";
    let wait_start_time = Instant::now();
    loop {
        match reqwest::blocking::get(server_status_url) {
            Ok(_) => break,
            Err(_) => info!("Waiting for the server to start."),
        };
        if wait_start_time.elapsed() > timeout {
            return Err(anyhow!("Error while starting server, timeout exceeded."));
        }
        thread::sleep(Duration::from_millis(1000));
    }
    Ok(())
}

// has to be public as long as docker is enabled externally, not through this crate
pub fn start_server_msg_listener() {
    thread::Builder::new()
        .name("Websocket Thread".to_string())
        .spawn(|| {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async { server_msg_listener().await });
        })
        .unwrap();
}

async fn server_msg_listener() {
    let url = format!("ws://127.0.0.1:{}/ws", read_config().api_port);

    let (ws_stream, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("Failed to connect");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (mut outgoing, mut incoming) = ws_stream.split();

    let sender_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let tungstenite::Message::Close(None) = &msg {
                let _ = outgoing.send(msg).await;
                return;
            }
            match outgoing.send(msg).await {
                Ok(()) => (),
                Err(e) => {
                    println!("Send Loop: {:?}", e);
                    let _ = outgoing.send(tungstenite::Message::Close(None)).await;
                    return;
                }
            }
        }
    });

    let receiver_task = tokio::spawn(async move {
        while let Some(result) = incoming.next().await {
            match result {
                Ok(tungstenite::Message::Close(_)) => {
                    let _ = tx.send(tungstenite::Message::Close(None));
                    return;
                }
                Ok(tungstenite::Message::Ping(data)) => {
                    if tx.send(tungstenite::Message::Pong(data)).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx.send(tungstenite::Message::Close(None));
                    return;
                }
                _ => {
                    info!("Received compositor event: {:?}", result);
                }
            }
        }
    });

    sender_task.await.unwrap();
    receiver_task.await.unwrap();
}

fn log_request_error<T: Serialize + ?Sized>(request_body: &T, response: Response) {
    let status = response.status();
    let request_str = serde_json::to_string_pretty(request_body).unwrap();
    let body_str = response.text().unwrap();

    let formated_body = get_formated_body(&body_str);
    error!(
        "Request failed:\n\nRequest: {}\n\nResponse code: {}\n\nResponse body:\n{}",
        request_str, status, formated_body
    )
}

fn get_formated_body(body_str: &str) -> String {
    let Ok(mut body_json) = serde_json::from_str::<serde_json::Value>(body_str) else {
        return body_str.to_string();
    };

    let Some(stack_value) = body_json.get("stack") else {
        return serde_json::to_string_pretty(&body_json).unwrap();
    };

    let errors: Vec<&str> = stack_value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    let msg_string = " - ".to_string() + &errors.join("\n - ");
    let body_map = body_json.as_object_mut().unwrap();
    body_map.remove("stack");
    format!(
        "{}\n\nError stack:\n{}",
        serde_json::to_string_pretty(&body_map).unwrap(),
        msg_string,
    )
}

pub enum TestSample {
    /// 10 minute animated video with sound
    BigBuckBunnyH264Opus,
    /// 10 minute animated video with ACC encoded sound
    BigBuckBunnyH264AAC,
    /// 10 minute animated VP8 video with sound
    BigBuckBunnyVP8Opus,
    /// 10 minute animated VP9 video with sound
    BigBuckBunnyVP9Opus,
    /// 11 minute animated video with sound
    ElephantsDreamH264Opus,
    /// 11 minute animated VP8 video with sound
    ElephantsDreamVP8Opus,
    /// 11 minute animated VP9 video with sound
    ElephantsDreamVP9Opus,
    /// 28 sec video with no sound
    SampleH264,
    /// 28 sec VP8 video with no sound
    SampleVP8,
    /// 28 sec VP9 video with no sound
    SampleVP9,
    /// looped 28 sec video with no sound
    SampleLoopH264,
    /// generated sample video with no sound (also with second timer when using ffmpeg)
    TestPatternH264,
    /// generated sample VP8 video with no sound (also with second timer when using ffmpeg)
    TestPatternVP8,
    /// generated sample VP9 video with no sound (also with second timer when using ffmpeg)
    TestPatternVP9,
}

#[derive(Debug)]
struct AssetData {
    url: String,
    path: PathBuf,
}

pub fn download_all_assets() -> Result<()> {
    let assets = [AssetData {
        url: String::from("https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4"),
        path: examples_root_dir().join("examples/assets/BigBuckBunny.mp4"),
    },
    AssetData {
        url: String::from("http://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4"),
        path: examples_root_dir().join("examples/assets/ElephantsDream.mp4"),
    },
    AssetData {
        url: String::from("https://filesamples.com/samples/video/mp4/sample_1280x720.mp4"),
        path: examples_root_dir().join("examples/assets/sample_1280_720.mp4"),
    }];

    for asset in assets {
        if let Err(err) = download_asset(&asset) {
            warn!(?asset, "Error while downloading asset: {err}");
        }
        if let Err(err) = convert_to_webm(&asset.path, "vp9") {
            eprintln!("Error while converting video: {:?}", err);
        }
        if let Err(err) = convert_to_webm(&asset.path, "vp8") {
            eprintln!("Error while converting video: {:?}", err);
        }
    }

    Ok(())
}

fn convert_to_webm(input_path: &Path, codec: &str) -> Result<()> {
    let extension = match codec {
        "vp8" => "vp8.webm",
        "vp9" => "vp9.webm",
        _ => return Err(anyhow!("Unsupported codec: {}", codec)),
    };
    let output_path = input_path.with_extension(extension);
    if output_path.exists() {
        return Ok(());
    }

    let codec_args = match codec {
        "vp8" => vec![
            "-c:v", "libvpx", "-crf", "10", "-b:v", "2M", "-quality", "good",
        ],
        "vp9" => vec![
            "-c:v",
            "libvpx-vp9",
            "-crf",
            "10",
            "-b:v",
            "2M",
            "-quality",
            "good",
        ],
        _ => return Err(anyhow!("Unsupported codec: {}", codec)),
    };
    let status = Command::new("ffmpeg")
        .arg("-i")
        .arg(
            input_path
                .to_str()
                .ok_or_else(|| anyhow!("Invalid input path"))?,
        )
        .args(codec_args)
        .arg(
            output_path
                .to_str()
                .ok_or_else(|| anyhow!("Invalid output path"))?,
        )
        .status()?;

    if !status.success() {
        return Err(anyhow::Error::msg(format!(
            "ffmpeg failed to convert mp4 {input_path:?} to webm",
        )));
    }

    Ok(())
}

fn map_asset_to_path(asset: &TestSample) -> Option<PathBuf> {
    match asset {
        TestSample::BigBuckBunnyH264Opus | TestSample::BigBuckBunnyH264AAC => {
            Some(examples_root_dir().join("examples/assets/BigBuckBunny.mp4"))
        }
        TestSample::BigBuckBunnyVP8Opus => {
            Some(examples_root_dir().join("examples/assets/BigBuckBunny.vp8.webm"))
        }
        TestSample::BigBuckBunnyVP9Opus => {
            Some(examples_root_dir().join("examples/assets/BigBuckBunny.vp9.webm"))
        }
        TestSample::ElephantsDreamH264Opus => {
            Some(examples_root_dir().join("examples/assets/ElephantsDream.mp4"))
        }
        TestSample::ElephantsDreamVP8Opus => {
            Some(examples_root_dir().join("examples/assets/ElephantsDream.vp8.webm"))
        }
        TestSample::ElephantsDreamVP9Opus => {
            Some(examples_root_dir().join("examples/assets/ElephantsDream.vp9.webm"))
        }
        TestSample::SampleH264 | TestSample::SampleLoopH264 => {
            Some(examples_root_dir().join("examples/assets/sample_1280_720.mp4"))
        }
        TestSample::SampleVP8 => {
            Some(examples_root_dir().join("examples/assets/sample_1280_720.vp8.webm"))
        }
        TestSample::SampleVP9 => {
            Some(examples_root_dir().join("examples/assets/sample_1280_720.vp9.webm"))
        }
        TestSample::TestPatternH264 | TestSample::TestPatternVP8 | TestSample::TestPatternVP9 => {
            None
        }
    }
}

pub fn get_asset_path(asset: TestSample) -> Result<PathBuf> {
    let path = map_asset_to_path(&asset).unwrap();
    match ensure_asset_available(&path) {
        Ok(()) => Ok(path),
        Err(e) => Err(e),
    }
}

fn ensure_asset_available(asset_path: &PathBuf) -> Result<()> {
    if !asset_path.exists() {
        return Err(anyhow!(
            "asset under path {:?} does not exist, try downloading it again",
            asset_path
        ));
    }
    Ok(())
}

pub fn examples_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn download_file(url: &str, path: &str) -> Result<PathBuf> {
    let sample_path = env::current_dir()?.join(path);
    fs::create_dir_all(sample_path.parent().unwrap())?;

    if sample_path.exists() {
        return Ok(sample_path);
    }

    let mut resp = reqwest::blocking::get(url)?;
    let mut out = File::create(sample_path.clone())?;
    io::copy(&mut resp, &mut out)?;
    Ok(sample_path)
}

fn download_asset(asset: &AssetData) -> Result<()> {
    fs::create_dir_all(asset.path.parent().unwrap())?;
    if !asset.path.exists() {
        let mut resp = reqwest::blocking::get(&asset.url)?;
        let mut out = File::create(asset.path.clone())?;
        io::copy(&mut resp, &mut out)?;
    }
    Ok(())
}
