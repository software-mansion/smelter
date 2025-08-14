use serde_json::json;

use crate::smelter_state::outputs::{AudioEncoder, OutputHandler, VideoEncoder, VideoResolution};

pub struct RtmpOutput {
    name: String,
    url: String,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
}

impl OutputHandler for RtmpOutput {
    fn name(&self) -> &str {
        &self.name
    }

    fn serialize_register(&self, inputs: &[&str]) -> serde_json::Value {
        json!({
            "type": "rtmp_client",
            "url": self.url,

        })
    }
}

pub struct RtmpOutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
}

impl RtmpOutputVideoOptions {
    pub fn serialize_register(&self, inputs: &[&str]) -> serde_json::Value {
        let input_json = inputs
            .iter()
            .map(|input_id| {
                json!({
                    "type": "input_stream",
                    "id": input_id,
                    "input_id": input_id,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "root": {
                "type": "tiles",
                "id": "tiles",
                "transition": {
                    "duration_ms": 500,
                },
                "children": input_json,
            },
        })
    }
}

impl Default for RtmpOutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        Self {
            resolution,
            encoder: VideoEncoder::FfmpegH264,
        }
    }
}

pub struct RtmpOutputAudioOptions {
    encoder: AudioEncoder,
}

impl Default for RtmpOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Aac,
        }
    }
}
