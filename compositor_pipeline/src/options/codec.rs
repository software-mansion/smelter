pub mod ffmpeg_h264 {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct EncoderOptions {
        pub preset: EncoderPreset,
        pub resolution: compositor_render::Resolution,
        pub raw_options: Vec<(String, String)>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum EncoderPreset {
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
}

pub mod ffmpeg_vp8 {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct EncoderOptions {
        pub resolution: compositor_render::Resolution,
        pub raw_options: Vec<(String, String)>,
    }
}

pub mod ffmpeg_vp9 {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct EncoderOptions {
        pub resolution: compositor_render::Resolution,
        pub raw_options: Vec<(String, String)>,
    }
}

pub mod opus {
    use crate::*;

    #[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
    pub enum EncoderPreset {
        Quality,
        Voip,
        LowestLatency,
    }

    #[derive(Debug, Clone, Eq, PartialEq, Hash)]
    pub struct EncoderOptions {
        pub channels: AudioChannels,
        pub preset: EncoderPreset,
        pub sample_rate: u32,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DecoderOptions {
        pub forward_error_correction: bool,
    }
}

pub mod fdk_aac {
    use crate::*;

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    pub struct EncoderOptions {
        pub channels: AudioChannels,
        pub sample_rate: u32,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DecoderOptions {
        pub asc: Option<bytes::Bytes>,
    }
}
