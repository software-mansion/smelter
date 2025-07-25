use std::sync::Arc;

use crate::codecs::{AudioEncoderOptions, VideoEncoderOptions};

#[derive(Debug, Clone)]
pub struct RtmpInputOptions {
    pub url: Arc<str>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}
