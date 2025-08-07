use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::codecs::{AudioEncoderOptions, VideoCodec, VideoDecoderOptions, VideoEncoderOptions};

#[derive(Debug, Clone)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub video_decoders: HashMap<VideoCodec, VideoDecoderOptions>,
}

#[derive(Debug, Clone)]
pub struct HlsOutputOptions {
    pub output_path: PathBuf,
    pub max_playlist_size: Option<usize>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}
