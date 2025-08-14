use crate::smelter_state::outputs::{AudioEncoder, VideoEncoder, VideoResolution};

pub struct RtmpOutput {
    name: String,
    url: String,
    video: Option<RtmpOutputVideoOptions>,
    audio: Option<RtmpOutputAudioOptions>,
}

pub struct RtmpOutputVideoOptions {
    resolution: VideoResolution,
    encoder: VideoEncoder,
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
