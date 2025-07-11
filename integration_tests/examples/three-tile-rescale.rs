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

const IP: &str = "127.0.0.1";
const INPUT_1_VIDEO_PORT: u16 = 8402;
const INPUT_1_AUDIO_PORT: u16 = 8404;
const INPUT_2_VIDEO_PORT: u16 = 8406;
const INPUT_2_AUDIO_PORT: u16 = 8408;
const INPUT_3_VIDEO_PORT: u16 = 8410;
const INPUT_3_AUDIO_PORT: u16 = 8412;

const OUTPUT_VIDEO_PORT: u16 = 8452;
const OUTPUT_AUDIO_PORT: u16 = 8454;

const PATH: &str = env!("CARGO_MANIFEST_DIR");

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1280,
    height: 720,
};

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
            "libopus",
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

    examples::post(
        "input/input_1_video/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_1_VIDEO_PORT,
            "video": {
                "decoder": "ffmpeg_h264",
            },
        }),
    )?;

    examples::post(
        "input/input_2_video/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_2_VIDEO_PORT,
            "video": {
                "decoder": "ffmpeg_h264",
            },
        }),
    )?;

    examples::post(
        "input/input_3_video/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_3_VIDEO_PORT,
            "video": {
                "decoder": "ffmpeg_h264",
            },
        }),
    )?;

    examples::post(
        "input/input_1_audio/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_1_AUDIO_PORT,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_2_audio/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_2_AUDIO_PORT,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_3_audio/register",
        &json!({
            "type": "rtp_stream",
            "port": INPUT_3_AUDIO_PORT,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    let scene1_video = json!({
        "type": "view",
        "background_color": "#000000FF",
        "children": [
            {
                "type": "view",
                "id": "scaled",
                "width": VIDEO_RESOLUTION.width,
                "height": VIDEO_RESOLUTION.height,
                "top": 0,
                "left": 0,
                "transition": {
                    "duration_ms": 2500.0,
                },
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fit",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_1_video",
                        },
                    },
                ],
            },
        ],
    });

    let scene1_audio = json!({
        "inputs": [
            {
                "input_id": "input_1_audio",
                "volume": 1.0f32,
            },
            {
                "input_id": "input_2_audio",
                "volume": 0.0f32,
            },
            {
                "input_id": "input_3_audio",
                "volume": 0.0f32,
            },
        ],
    });

    let scene1to2_video = json!({
        "type": "view",
        "background_color": "#000000FF",
        "children": [
            {
                "type": "view",
                "id": "scaled",
                "width": 1,
                "height": 1,
                "top": 0,
                "left": 0,
                "transition": {
                    "duration_ms": 2500.0,
                },
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fit",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_1_video",
                        }
                    },
                ],
            },
        ],
    });

    let scene2_video = json!({
        "type": "view",
        "background_color": "#000000FF",
        "children": [
            {
                "type": "view",
                "id": "scaled",
                "width": VIDEO_RESOLUTION.width,
                "height": VIDEO_RESOLUTION.height,
                "top": 0,
                "right": 0,
                "transition": {
                    "duration_ms": 2500.0,
                },
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fit",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_2_video",
                        }
                    },
                ],
            },
        ],
    });

    let scene2to3_video = json!({
        "type": "view",
        "background_color": "#000000FF",
        "children": [
            {
                "type": "view",
                "id": "scaled",
                "width": 1,
                "height": 1,
                "top": 0,
                "right": 0,
                "transition": {
                    "duration_ms": 2500.0,
                },
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fit",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_2_video",
                        }
                    },
                ],
            },
        ],
    });

    let scene2_audio = json!({
        "inputs": [
            {
                "input_id": "input_1_audio",
                "volume": 0.0f32,
            },
            {
                "input_id": "input_2_audio",
                "volume": 1.0f32,
            },
            {
                "input_id": "input_3_audio",
                "volume": 0.0f32,
            },
        ],
    });

    let scene3_video = json!({
        "type": "view",
        "background_color": "#000000FF",
        "children": [
            {
                "type": "view",
                "id": "scaled",
                "width": VIDEO_RESOLUTION.width,
                "height": VIDEO_RESOLUTION.height,
                "bottom": 0,
                "left": 0,
                "transition": {
                    "duration_ms": 2500.0,
                },
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fit",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_3_video",
                        }
                    },
                ],
            },
        ],
    });

    let scene3_audio = json!({
        "inputs": [
            {
                "input_id": "input_1_audio",
                "volume": 0.0f32,
            },
            {
                "input_id": "input_2_audio",
                "volume": 0.0f32,
            },
            {
                "input_id": "input_3_audio",
                "volume": 1.0f32,
            },
        ],
    });

    let scene3to1_video = json!({
        "type": "view",
        "background_color": "#000000FF",
        "children": [
            {
                "type": "view",
                "id": "scaled",
                "width": 1,
                "height": 1,
                "bottom": 0,
                "left": 0,
                "transition": {
                    "duration_ms": 2500.0,
                },
                "children": [
                    {
                        "type": "rescaler",
                        "mode": "fit",
                        "child": {
                            "type": "input_stream",
                            "input_id": "input_3_video",
                        }
                    },
                ],
            },
        ],
    });

    examples::post(
        "output/output_video/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_VIDEO_PORT,
            "video": {
                "resolution": { "width": VIDEO_RESOLUTION.width, "height": VIDEO_RESOLUTION.height },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                },
                "initial": {
                    "root": scene1_video,
                },
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
                "initial": scene1_audio,
            },
        }),
    )?;

    let path_1 = Path::new(PATH).join("examples/assets/lachrymaQuiet.mp4");
    let path_2 = Path::new(PATH).join("examples/assets/peacefieldQuiet.mp4");
    let path_3 = Path::new(PATH).join("examples/assets/satanizedQuiet.mp4");

    std::thread::sleep(Duration::from_millis(1000));

    examples::post("start", &json!({}))?;

    // VIDEO STREAMS
    // stream 1 - lachryma
    ffmpeg_video_stream_from_file(&path_1, IP, INPUT_1_VIDEO_PORT)?;

    // stream 2 - peacefield
    ffmpeg_video_stream_from_file(&path_2, IP, INPUT_2_VIDEO_PORT)?;

    // stream 3 - satanized
    ffmpeg_video_stream_from_file(&path_3, IP, INPUT_3_VIDEO_PORT)?;

    // AUDIO STREAMS
    // stream 1 - lachryma
    ffmpeg_audio_stream_from_file(&path_1, IP, INPUT_1_AUDIO_PORT)?;

    // stream 2 - peacefield
    ffmpeg_audio_stream_from_file(&path_2, IP, INPUT_2_AUDIO_PORT)?;

    // stream 3 - satanized
    ffmpeg_audio_stream_from_file(&path_3, IP, INPUT_3_AUDIO_PORT)?;

    std::thread::sleep(Duration::from_secs(10));

    loop {
        examples::post(
            "output/output_video/update",
            &json!({
                "video": {
                    "root": scene1to2_video,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_millis(2500));

        examples::post(
            "output/output_video/update",
            &json!({
                "video": {
                    "root": scene2_video,
                },
            }),
        )?;

        examples::post(
            "output/output_audio/update",
            &json!({
                "audio": scene2_audio,
            }),
        )?;

        std::thread::sleep(Duration::from_secs(10));

        examples::post(
            "output/output_video/update",
            &json!({
                "video": {
                    "root": scene2to3_video,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_millis(2500));

        examples::post(
            "output/output_video/update",
            &json!({
                "video": {
                    "root": scene3_video,
                },
            }),
        )?;

        examples::post(
            "output/output_audio/update",
            &json!({
                "audio": scene3_audio,
            }),
        )?;

        std::thread::sleep(Duration::from_secs(10));

        examples::post(
            "output/output_video/update",
            &json!({
                "video": {
                    "root": scene3to1_video,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_millis(2500));

        examples::post(
            "output/output_video/update",
            &json!({
                "video": {
                    "root": scene1_video,
                },
            }),
        )?;

        examples::post(
            "output/output_audio/update",
            &json!({
                "audio": scene1_audio,
            }),
        )?;

        std::thread::sleep(Duration::from_secs(10));
    }

    #[allow(unreachable_code)]
    Ok(())
}
