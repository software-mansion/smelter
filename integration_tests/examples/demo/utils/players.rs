use strum::{Display, EnumIter};

#[derive(Debug, EnumIter, Display, Clone)]
pub enum RtpOutputPlayerOptions {
    #[strum(to_string = "Start FFmpeg receiver")]
    StartFfmpegReceiver,

    #[strum(to_string = "Done")]
    Done,
}
