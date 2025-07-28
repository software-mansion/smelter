use compositor_render::Resolution;

use crate::codecs::OutputPixelFormat;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FfmpegVp9EncoderOptions {
    pub resolution: Resolution,
    pub pixel_format: OutputPixelFormat,
    pub raw_options: Vec<(String, String)>,
}
