use std::{path::Path, sync::Arc, time::Duration};

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub decoder_options: HlsInputDecoders,
    pub queue_options: QueueInputOptions,
    /// Ignored for live playlists, where the live edge decides where the
    /// playback starts.
    pub offset: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HlsOutputOptions {
    pub output_path: Arc<Path>,
    pub max_playlist_size: Option<usize>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
    pub raw_options: Vec<(Arc<str>, Arc<str>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HlsInputDecoders {
    pub h264: Option<VideoDecoderOptions>,
}
