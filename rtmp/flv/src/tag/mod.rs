pub mod audio;
pub mod video;

pub mod scriptdata;

/// Information if tag contains av data or decoder config.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketType {
    Data,

    /// This field is valid only for AVC for video and AAC for audio.
    Config,
}
