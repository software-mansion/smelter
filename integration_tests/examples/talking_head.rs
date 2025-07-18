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
const INPUT_PORT_0: u16 = 8002;
const INPUT_PORT_1: u16 = 8004;
const INPUT_PORT_2: u16 = 8006;
const INPUT_PORT_3: u16 = 8008;
const INPUT_PORT_4: u16 = 8010;
const INPUT_PORT_5: u16 = 8012;
const INPUT_PORT_6: u16 = 8014;
const INPUT_PORT_7: u16 = 8016;
const INPUT_PORT_8: u16 = 8018;
const INPUT_PORT_9: u16 = 8020;

const OUTPUT_PORT: u16 = 8022;

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
    start_ffmpeg_receive_h264(None, Some(OUTPUT_PORT))?;

    examples::post(
        "input/input_0/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_0,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

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

    examples::post(
        "input/input_6/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_6,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_7/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_7,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_8/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_8,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    examples::post(
        "input/input_9/register",
        &json!({
            "type": "rtp_stream",
            "transport_protocol": "udp",
            "port": INPUT_PORT_9,
            "audio": {
                "decoder": "opus",
            },
        }),
    )?;

    let scene0 = json!([
        {
            "input_id": "input_0",
        },
    ]);

    let scene1 = json!([
        {
            "input_id": "input_0",
        },
        {
            "input_id": "input_1",
        },
    ]);

    let scene2 = json!([
        {
            "input_id": "input_0",
        },
        {
            "input_id": "input_1",
        },
        {
            "input_id": "input_2",
        },
    ]);

    let scene3 = json!([
        {
            "input_id": "input_0",
        },
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
            "input_id": "input_0",
        },
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
            "input_id": "input_0",
        },
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

    let scene6 = json!([
        {
            "input_id": "input_0",
        },
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
        {
            "input_id": "input_6",
        },
    ]);

    let scene7 = json!([
        {
            "input_id": "input_0",
        },
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
        {
            "input_id": "input_6",
        },
        {
            "input_id": "input_7",
        },
    ]);

    let scene8 = json!([
        {
            "input_id": "input_0",
        },
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
        {
            "input_id": "input_6",
        },
        {
            "input_id": "input_7",
        },
        {
            "input_id": "input_8",
        },
    ]);

    let scene9 = json!([
        {
            "input_id": "input_0",
        },
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
        {
            "input_id": "input_6",
        },
        {
            "input_id": "input_7",
        },
        {
            "input_id": "input_8",
        },
        {
            "input_id": "input_9",
        },
    ]);

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_PORT,
            "audio": {
                "channels": "stereo",
                "mixing_strategy": "sum_scale",
                "encoder": {
                    "type": "opus",
                    "sample_rate": 48000,
                    "preset": "quality",
                },
                "initial": {
                    "inputs": scene0,
                },
            },
        }),
    )?;

    let path0 = Path::new(PATH).join("examples/assets/talkingHead/talkingHead.mp4");
    let path1 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset1m.mp4");
    let path2 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset2m.mp4");
    let path3 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset3m.mp4");
    let path4 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset4m.mp4");
    let path5 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset5m.mp4");
    let path6 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset6m.mp4");
    let path7 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset7m.mp4");
    let path8 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset8m.mp4");
    let path9 = Path::new(PATH).join("examples/assets/talkingHead/talkingHeadOffset9m.mp4");

    ffmpeg_audio_stream_from_file(&path0, IP, INPUT_PORT_0)?;
    ffmpeg_audio_stream_from_file(&path1, IP, INPUT_PORT_1)?;
    ffmpeg_audio_stream_from_file(&path2, IP, INPUT_PORT_2)?;
    ffmpeg_audio_stream_from_file(&path3, IP, INPUT_PORT_3)?;
    ffmpeg_audio_stream_from_file(&path4, IP, INPUT_PORT_4)?;
    ffmpeg_audio_stream_from_file(&path5, IP, INPUT_PORT_5)?;
    ffmpeg_audio_stream_from_file(&path6, IP, INPUT_PORT_6)?;
    ffmpeg_audio_stream_from_file(&path7, IP, INPUT_PORT_7)?;
    ffmpeg_audio_stream_from_file(&path8, IP, INPUT_PORT_8)?;
    ffmpeg_audio_stream_from_file(&path9, IP, INPUT_PORT_9)?;

    std::thread::sleep(Duration::from_millis(500));

    examples::post("start", &json!({}))?;

    std::thread::sleep(Duration::from_secs(5));

    loop {
        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene1,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene2,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene3,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene4,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene5,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene6,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene7,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene8,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene9,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene8,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene7,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene6,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene5,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene4,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene3,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene2,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene1,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));

        examples::post(
            "output/output_1/update",
            &json!({
                "audio": {
                    "inputs": scene0,
                },
            }),
        )?;

        std::thread::sleep(Duration::from_secs(5));
    }

    #[allow(unreachable_code)]
    Ok(())
}
