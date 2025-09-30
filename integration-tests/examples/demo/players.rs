use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

#[derive(Debug, EnumIter, Display, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OutputPlayer {
    #[strum(to_string = "FFmpeg")]
    Ffmpeg,

    #[strum(to_string = "GStreamer")]
    Gstreamer,

    #[strum(to_string = "Manual")]
    Manual,
}

#[derive(Debug, EnumIter, Display, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputPlayer {
    #[strum(to_string = "FFmpeg")]
    Ffmpeg,

    #[strum(to_string = "GStreamer")]
    Gstreamer,

    #[strum(to_string = "Manual")]
    Manual,
}
