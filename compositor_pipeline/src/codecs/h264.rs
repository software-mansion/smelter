use compositor_render::Resolution;

use crate::codecs::OutputPixelFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FfmpegH264EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
    Placebo,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FfmpegH264EncoderOptions {
    pub preset: FfmpegH264EncoderPreset,
    pub resolution: Resolution,
    pub pixel_format: OutputPixelFormat,
    pub raw_options: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum H264AvcDecoderConfigError {
    #[error("Incorrect AVCDecoderConfig. Expected more bytes.")]
    NotEnoughBytes(#[from] bytes::TryGetError),

    #[error("Not AVCC")]
    NotAVCC,
}
