use std::{path::PathBuf, sync::Arc, time::Duration};

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};

#[derive(Debug, Clone)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub video_decoders: HlsInputVideoDecoders,

    /// Duration of stream that should be buffered before stream is started.
    /// If you have both audio and video streams then make sure to use the same value
    /// to avoid desync.
    ///
    /// This value defines minimal latency on the queue, but if you set it to low and fail
    /// to deliver the input stream on time it can cause either black screen or flickering image.
    ///
    /// By default DEFAULT_BUFFER_DURATION will be used.
    pub buffer_duration: Option<Duration>,
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
