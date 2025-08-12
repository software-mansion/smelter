use strum::{Display, EnumIter};

#[derive(Debug, EnumIter, Display, Clone)]
pub enum OutputPlayerOptions {
    #[strum(to_string = "Start FFmpeg receiver")]
    StartFfmpegReceiver,

    #[strum(to_string = "Manual")]
    Manual,
}

#[derive(Debug, EnumIter, Display, Clone)]
pub enum InputPlayerOptions {
    #[strum(to_string = "Start FFmpeg transmitter")]
    StartFfmpegTransmitter,

    #[strum(to_string = "Start GStreamer transmitter")]
    StartGstreamerTransmitter,

    #[strum(to_string = "Manual")]
    Manual,
}
