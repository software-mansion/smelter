use anyhow::Result;
use integration_tests::examples::run_example;

fn main() {
    run_example(client_code);
}

#[cfg(target_os = "macos")]
fn client_code() -> Result<()> {
    panic!("Your OS does not support vulkan");
}

#[cfg(target_os = "linux")]
fn client_code() -> Result<()> {
    use serde_json::json;
    use smelter_api::Resolution;

    use integration_tests::{
        examples,
        media::{MediaReceiver, MediaSender, Receive, Send, TestSample, VideoCodec},
    };

    const VIDEO_RESOLUTION: Resolution = Resolution {
        width: 1280,
        height: 720,
    };

    const IP: &str = "127.0.0.1";
    const INPUT_PORT: u16 = 8006;
    const OUTPUT_PORT: u16 = 8004;

    const VIDEOS: u16 = 6;
    MediaReceiver::new(Receive::rtp_udp_listener().video(OUTPUT_PORT, VideoCodec::H264)).spawn()?;

    let mut children = Vec::new();

    for i in 0..VIDEOS {
        let input_id = format!("input_{i}");

        examples::post(
            &format!("input/{input_id}/register"),
            &json!({
                "type": "rtp_stream",
                "port": INPUT_PORT + i * 2,
                "video": {
                    "decoder": "vulkan_h264"
                }
            }),
        )?;

        children.push(json!({
            "type": "input_stream",
            "input_id": input_id,
        }));
    }

    let scene = json!({
        "type": "tiles",
        "id": "tile",
        "padding": 5,
        "background_color": "#444444FF",
        "children": children,
    });

    examples::post(
        "output/output_1/register",
        &json!({
            "type": "rtp_stream",
            "ip": IP,
            "port": OUTPUT_PORT,
            "video": {
                "resolution": {
                    "width": VIDEO_RESOLUTION.width,
                    "height": VIDEO_RESOLUTION.height,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                },
                "initial": {
                    "root": scene,
                },
            },
        }),
    )?;

    examples::post("start", &json!({}))?;

    for i in 0..VIDEOS {
        MediaSender::new(
            TestSample::BigBuckBunnyH264Opus,
            Send::rtp_udp_client().video_port(INPUT_PORT + 2 * i),
        )
        .spawn()?;
    }

    Ok(())
}
