use std::{path::Path, sync::Arc, time::Duration};

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};

#[derive(Debug, Clone)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub video_decoders: HlsInputVideoDecoders,
    pub required: bool,
    pub offset: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct HlsOutputOptions {
    pub output_path: Arc<Path>,
    pub max_playlist_size: Option<usize>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct HlsInputVideoDecoders {
    pub h264: Option<VideoDecoderOptions>,
}
