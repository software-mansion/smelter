use std::sync::Arc;

use smelter_render::{Framerate, Resolution};

#[derive(Debug, Clone)]
pub struct V4L2InputOptions {
    pub path: Arc<std::path::Path>,
    pub resolution: Resolution,
    pub format: V4l2Format,
    pub framerate: Framerate,
}

#[derive(Debug, Clone, Copy)]
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
