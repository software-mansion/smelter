use std::sync::Arc;

use smelter_render::Resolution;

use crate::codecs::VideoEncoderBitrate;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FfmpegVp8EncoderOptions {
    pub bitrate: Option<VideoEncoderBitrate>,
    pub keyframe_interval_ms: u64,
    pub resolution: Resolution,
    pub raw_options: Vec<(Arc<str>, Arc<str>)>,
}
