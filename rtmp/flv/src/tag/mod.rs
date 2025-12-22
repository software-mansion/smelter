pub mod audio;
pub mod video;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketType {
    Data,
    Config,
}
