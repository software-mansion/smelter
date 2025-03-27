use anyhow::Result;
use compositor_api::types::Resolution;
use serde_json::json;

use integration_tests::{
    examples::{self, run_example},
    ffmpeg::start_ffmpeg_receive,
};

const VIDEO_RESOLUTION: Resolution = Resolution {
    width: 1920,
    height: 1080,
};

const IP: &str = "127.0.0.1";
const OUTPUT_PORT: u16 = 8002;

fn main() {
    run_example(client_code);
}

fn client_code() -> Result<()> {
    start_ffmpeg_receive(Some(OUTPUT_PORT), None)?;

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
                    "preset": "ultrafast"
                },
                "initial": {
                    "root": {
                        "type": "text",
                        "text": "VideoCompositor🚀\nSecond Line\nLorem ipsum dolor sit amet consectetur adipisicing elit. Soluta delectus optio fugit maiores eaque ab totam, veritatis aperiam provident, aliquam consectetur deserunt cumque est? Saepe tenetur impedit culpa asperiores id?",
                        "font_size": 100.0,
                        "font_family": "Comic Sans MS",
                        "align": "center",
                        "wrap": "word",
                        "background_color": "#00800000",
                        "weight": "bold",
                        "width": VIDEO_RESOLUTION.width,
                        "height": VIDEO_RESOLUTION.height,
                    }
                }
            }
        }),
    )?;

    examples::post("start", &json!({}))?;

    Ok(())
}
