use std::{thread, time::Duration};

use crate::{
    compare_video_dumps, input_dump_from_disk, CommunicationProtocol, CompositorInstance,
    OutputReceiver, PacketSender, VideoValidationConfig,
};
use anyhow::Result;
use serde_json::json;

/// Setup scene with stream and image and send unregister request after 4 seconds for both.
///
/// Show image and stream for 4 seconds. After that only image is displayed.
#[test]
pub fn unregistering() -> Result<()> {
    const OUTPUT_DUMP_FILE: &str = "unregistering_test_output.rtp";
    let instance = CompositorInstance::start(None);
    let input_port = instance.get_port();
    let output_port = instance.get_port();

    // This should fail because image is not registered yet.
    register_output_with_initial_scene(&instance, output_port)
        .expect_err("Image has to be registered first");

    instance.send_request(
        "image/image_1/register",
        json!({
            "asset_type": "svg",
            "path": format!("{}/assets/image.svg", env!("CARGO_MANIFEST_DIR"))
        }),
    )?;

    register_output_with_initial_scene(&instance, output_port)?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20000,
        }),
    )?;

    let output_receiver = OutputReceiver::start(output_port, CommunicationProtocol::Tcp)?;

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": input_port,
            "video": {
                "decoder": "ffmpeg_h264"
            },
        }),
    )?;

    let input_1_dump = input_dump_from_disk("8_colors_long_input_video.rtp")?;
    PacketSender::new(CommunicationProtocol::Tcp, input_port)?.send_non_blocking(input_1_dump);

    instance.send_request("start", json!({}))?;

    thread::sleep(Duration::from_secs(4));

    instance.send_request("input/input_1/unregister", json!({}))?;

    instance.send_request("image/image_1/unregister", json!({}))?;

    let new_output_dump = output_receiver.wait_for_output()?;

    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        VideoValidationConfig {
            validation_intervals: vec![Duration::from_secs(0)..Duration::from_secs(10)],
            allowed_invalid_frames: 1,
            ..Default::default()
        },
    )?;

    Ok(())
}

fn register_output_with_initial_scene(instance: &CompositorInstance, port: u16) -> Result<()> {
    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "rtp_stream",
            "transport_protocol": "tcp_server",
            "port": port,
            "video": {
                "resolution": {
                    "width": 640,
                    "height": 360,
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast"
                },
                "initial": {
                    "root": {
                        "type": "tiles",
                        "padding": 3,
                        "background_color": "#DDDDDDFF",
                        "children": [
                            {
                                "type": "input_stream",
                                "input_id": "input_1",
                            },
                            {
                                "type": "image",
                                "image_id": "image_1",
                            },
                        ],
                    }
                }
            },
        }),
    )
}
