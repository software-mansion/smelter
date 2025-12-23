use bytes::Bytes;

use crate::tag::PacketType;

/// Struct representing flv VIDEODATA.
#[derive(Debug, Clone)]
pub struct VideoTag {
    pub packet_type: PacketType,
    pub codec: VideoCodec,

    /// AVC config only. Composition time offset.
    pub composition_time: Option<i32>,
    pub frame_type: FrameType,
    pub data: Bytes,
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

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum FrameType {
    #[default]
    Keyframe,
    Interframe,
}
