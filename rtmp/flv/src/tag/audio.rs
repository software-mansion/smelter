use bytes::Bytes;

use crate::PacketType;

#[derive(Debug, Clone)]
pub struct AudioTag {
    pub packet_type: PacketType,
    pub codec: AudioCodec,
    pub codec_params: AudioCodecParams,
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

#[derive(Debug, Clone, Default)]
pub struct AudioCodecParams {
    pub sound_rate: u32,
    pub sound_type: AudioChannels,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum AudioChannels {
    Mono,

    #[default]
    Stereo,
}
