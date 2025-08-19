use crate::smelter_state::outputs::{AudioEncoder, VideoEncoder, VideoResolution};

const WHIP_PORT_ENV: &str = "WHIP_PORT";
const WHIP_URL_ENV: &str = "WHIP_URL";

pub struct WhipOutput {
    name: String,
    endpoint_url: String,
    bearer_token: Option<String>,
    video: Option<WhipOutputVideoOptions>,
    audio: Option<WhipOutputAudioOptions>,
}

pub struct WhipOutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
}

impl Default for WhipOutputVideoOptions {
    fn default() -> Self {
        let resolution = VideoResolution {
            width: 1920,
            height: 1080,
        };
        Self {
            resolution,
            encoder: VideoEncoder::Any,
        }
    }
}

pub struct WhipOutputAudioOptions {
    encoder: AudioEncoder,
}

impl Default for WhipOutputAudioOptions {
    fn default() -> Self {
        Self {
            encoder: AudioEncoder::Any,
        }
    }
}
