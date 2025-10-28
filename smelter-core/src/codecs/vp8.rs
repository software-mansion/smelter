use std::sync::Arc;

use smelter_render::Resolution;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FfmpegVp8EncoderOptions {
    pub resolution: Resolution,
    pub raw_options: Vec<(Arc<str>, Arc<str>)>,
}
