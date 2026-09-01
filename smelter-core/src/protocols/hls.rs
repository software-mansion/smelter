use std::{path::Path, sync::Arc, time::Duration};

use crate::codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions};
use crate::protocols::LiveInputBufferOptions;
use crate::queue::QueueInputOptions;

#[derive(Debug, Clone, PartialEq)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
    pub decoder_options: HlsInputDecoders,
    pub queue_options: QueueInputOptions,
    /// Ignored for live playlists, where the live edge decides where the
    /// playback starts.
    pub offset: Option<Duration>,
    /// For non-live playlists only the desired buffer is used to buffer at the
    /// start; `min`/`max` still shape the derived value when `desired` is not
    /// set, and `max` sizes the decoder channel.
    pub buffer: LiveInputBufferOptions,
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
pub struct HlsInputDecoders {
    pub h264: Option<VideoDecoderOptions>,
}
