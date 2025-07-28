use crate::codecs::{AudioEncoderOptions, VideoEncoderOptions};

#[derive(Debug, Clone)]
pub struct RtmpSenderOptions {
    pub url: String,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}
