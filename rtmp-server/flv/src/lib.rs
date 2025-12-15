pub mod parser;

pub use parser::{
    AudioChannels, AudioCodec, Codec, CodecParams, FrameType, Header, Packet, PacketType,
    ParseError, VideoCodec, parse_audio_payload, parse_video_payload,
};
