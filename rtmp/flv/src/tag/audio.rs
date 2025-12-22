use bytes::Bytes;

use crate::PacketType;

#[derive(Debug, Clone)]
pub struct AudioTag {
    pub packet_type: PacketType,
    pub codec: AudioCodec,
    pub sound_rate: u32,
    pub sound_type: AudioChannels,
    pub payload: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCodec {
    Pcm,
    Adpcm,
    Mp3,
    PcmLe,
    Nellymoser16kMono,
    Nellymoser8kMono,
    Nellymoser,
    G711ALaw,
    G711MuLaw,
    Aac,
    Speex,
    Mp3_8k,
    DeviceSpecific,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum AudioChannels {
    Mono,

    #[default]
    Stereo,
}
