use strum::{Display, EnumIter};

#[derive(Debug, EnumIter, Display, Clone, Copy, PartialEq)]
pub enum OutputPlayer {
    #[strum(to_string = "Start FFmpeg receiver")]
    FfmpegReceiver,

    #[strum(to_string = "Start GStreamer receiver")]
    GstreamerReceiver,

    #[strum(to_string = "Manual")]
    Manual,
}

#[derive(Debug, EnumIter, Display, Clone, Copy, PartialEq)]
pub enum InputPlayer {
    #[strum(to_string = "Start FFmpeg transmitter")]
    FfmpegTransmitter,

    #[strum(to_string = "Start GStreamer transmitter")]
    GstreamerTransmitter,

    #[strum(to_string = "Manual")]
    Manual,
}
