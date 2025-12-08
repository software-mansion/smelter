use std::sync::Arc;

use smelter_render::Resolution;

use crate::codecs::{OutputPixelFormat, VideoEncoderBitrate};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FfmpegVp9EncoderOptions {
    pub resolution: Resolution,
    pub bitrate: Option<VideoEncoderBitrate>,
    pub keyframe_interval: Option<u64>,
    pub pixel_format: OutputPixelFormat,
    pub raw_options: Vec<(Arc<str>, Arc<str>)>,
}
