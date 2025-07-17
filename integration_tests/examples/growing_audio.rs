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
const INPUT_PORT_1: u16 = 8002;
const INPUT_PORT_2: u16 = 8004;
const INPUT_PORT_3: u16 = 8006;
const INPUT_PORT_4: u16 = 8008;
const INPUT_PORT_5: u16 = 8010;
const OUTPUT_PORT_1: u16 = 8012;

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

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive_h264(None, Some(OUTPUT_PORT_1))?;

    examples::post(
        "input/input_1/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_1,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_2/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_2,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_3/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_3,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_4/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_4,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_5/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_5,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    let scene1 = json!([
        {
            "input_id": "input_1",
        },
    ]);

    let scene2 = json!([
        {
            "input_id": "input_1",
        },
        {
            "input_id": "input_2",
        },
    ]);

    let scene3 = json!([
        {
            "input_id": "input_1",
        },
        {
            "input_id": "input_2",
        },
        {
            "input_id": "input_3",
        },
    ]);

    let scene4 = json!([
        {
            "input_id": "input_1",
        },
        {
            "input_id": "input_2",
        },
        {
            "input_id": "input_3",
        },
        {
            "input_id": "input_4",
        },
    ]);

    let scene5 = json!([
        {
            "input_id": "input_1",
        },
        {
            "input_id": "input_2",
        },
        {
            "input_id": "input_3",
        },
        {
            "input_id": "input_4",
        },
        {
            "input_id": "input_5",
        },
    ]);

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_PORT_1,
            "audio": {
                "channels": "stereo",
                "mixing_strategy": "sum_scale",
                "encoder": {
                    "type": "opus",
                    "sample_rate": 48000,
                    "preset": "quality",
                },
                "initial": {
                    "inputs": scene1,
                },
            },
        }),
    )?;

    let path1 = Path::new(PATH).join("examples/assets/lachrymaSound.mp4");
    let path2 = Path::new(PATH).join("examples/assets/peacefieldSound.mp4");
    let path3 = Path::new(PATH).join("examples/assets/satanizedSound.mp4");
    let path4 = Path::new(PATH).join("examples/assets/kaisarionSound.mp4");
    let path5 = Path::new(PATH).join("examples/assets/spillwaysSound.mp4");

    ffmpeg_audio_stream_from_file(&path1, IP, INPUT_PORT_1)?;
    ffmpeg_audio_stream_from_file(&path2, IP, INPUT_PORT_2)?;
    ffmpeg_audio_stream_from_file(&path3, IP, INPUT_PORT_3)?;
    ffmpeg_audio_stream_from_file(&path4, IP, INPUT_PORT_4)?;
    ffmpeg_audio_stream_from_file(&path5, IP, INPUT_PORT_5)?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    std::thread::sleep(Duration::from_secs(5));

    loop {
        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene2,
                },
            }),
        )?;
        info!("2 streams playing!");

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene3,
                },
            }),
        )?;
        info!("3 streams playing!");

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene4,
                },
            }),
        )?;
        info!("4 streams playing!");

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene5,
                },
            }),
        )?;
        info!("5 streams playing!");

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene4,
                },
            }),
        )?;
        info!("4 streams playing!");

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene3,
                },
            }),
        )?;
        info!("3 streams playing!");

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene2,
                },
            }),
        )?;
        info!("2 streams playing!");

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene1,
                },
            }),
        )?;
        info!("1 stream playing!");

        std::thread::sleep(Duration::from_secs(5));
    }

    Ok(())
}
