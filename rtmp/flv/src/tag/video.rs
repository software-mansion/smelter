use bytes::Bytes;

use crate::PacketType;

#[derive(Debug, Clone)]
pub struct VideoTag {
    pub packet_type: PacketType,
    pub codec: VideoCodec,
    pub codec_params: VideoCodecParams,
    pub payload: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoCodec {
    SorensonH263,
    ScreenVideo,
    Vp6,
    Vp6WithAlpha,
    ScreenVideo2,
    H264,
}

#[derive(Debug, Clone, Default)]
pub struct VideoCodecParams {
    pub composition_time: i32,
    pub frame_type: FrameType,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum FrameType {
    #[default]
    Keyframe,
    Interframe,
}
