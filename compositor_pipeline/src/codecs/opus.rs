use crate::AudioChannels;

pub use opus::Error as LibOpusDecoderError;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct OpusEncoderOptions {
    pub channels: AudioChannels,
    pub preset: OpusEncoderPreset,
    pub sample_rate: u32,
    pub forward_error_correction: bool,
    pub packet_loss: i32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum OpusEncoderPreset {
    Quality,
    Voip,
    LowestLatency,
}
