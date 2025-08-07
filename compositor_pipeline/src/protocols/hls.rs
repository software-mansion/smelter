use std::{path::PathBuf, sync::Arc};

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};

#[derive(Debug, Clone)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub video_decoders: HlsInputVideoDecoders,
}

#[derive(Debug, Clone)]
pub struct HlsOutputOptions {
    pub output_path: PathBuf,
    pub max_playlist_size: Option<usize>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct HlsInputVideoDecoders {
    pub h264: VideoDecoderOptions,
}
