use std::{path::PathBuf, sync::Arc};

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};

#[derive(Debug, Clone)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub video_decoder: VideoDecoderOptions,
}

#[derive(Debug, Clone)]
pub struct HlsOutputOptions {
    pub output_path: PathBuf,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}
