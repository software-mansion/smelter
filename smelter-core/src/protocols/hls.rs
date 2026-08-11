use std::{path::Path, sync::Arc, time::Duration};

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub video_decoders: HlsInputVideoDecoders,
    pub queue_options: QueueInputOptions,
    pub offset: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HlsOutputOptions {
    pub output_path: Arc<Path>,
    pub max_playlist_size: Option<usize>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
    pub raw_options: Vec<(Arc<str>, Arc<str>)>,
    /// If set, the output is created immediately, but starts producing data only after
    /// the queue reaches this timestamp (relative to the queue start). It doubles as the
    /// timestamp offset of the produced playlist, so PTS 0 is exactly this moment rather
    /// than whenever the first chunk happened to be encoded.
    pub start_at: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HlsInputVideoDecoders {
    pub h264: Option<VideoDecoderOptions>,
}
