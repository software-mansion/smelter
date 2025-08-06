use std::fmt::Display;

pub mod rtp;

pub enum VideoDecoder {
    FfmpegH264,
    FfmpegVp8,
    FfmpegVp9,
}

impl Display for VideoDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::FfmpegH264 => "ffmpeg_h264",
            Self::FfmpegVp8 => "ffmpeg_vp8",
            Self::FfmpegVp9 => "ffmpeg_vp9",
        };
        write!(f, "{msg}")
    }
}

pub enum AudioDecoder {
    Opus,
    Aac,
}

impl Display for AudioDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Opus => "opus",
            Self::Aac => "aac",
        };
        write!(f, "{msg}")
    }
}
