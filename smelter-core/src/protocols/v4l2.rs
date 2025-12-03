use std::sync::Arc;

use smelter_render::{Framerate, Resolution};

#[derive(Debug, Clone)]
pub struct V4l2InputOptions {
    pub path: Arc<std::path::Path>,
    pub resolution: Option<Resolution>,
    pub format: V4l2Format,
    pub framerate: Option<Framerate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4l2Format {
    Yuyv,
    Nv12,
}

#[derive(Debug, thiserror::Error)]
pub enum V4l2InputError {
    #[error("Device does not support video capture")]
    CaptureNotSupported,

    #[error("Opening device {0} failed")]
    OpeningDeviceFailed(Arc<std::path::Path>, std::io::Error),

    #[error("Device IO error.")]
    IoError(#[from] std::io::Error),

    #[error("Device is set to an unsupported format: {0}.")]
    UnsupportedFormat(String),
}
