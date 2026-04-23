use bytes::{BufMut, Bytes, BytesMut};

use crate::{RtmpMessageSerializeError, error::FlvVideoTagParseError};

/// Struct representing legacy flv VIDEODATA.
/// Check <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=74> for more info.
#[derive(Debug, Clone)]
pub struct VideoTag {
    /// FrameType 4bits
    pub frame_type: VideoTagFrameType,
    /// CodecID 4bits
    pub codec: LegacyFlvVideoCodec,

    /// AVCPacketType 8bits IF CodecID == 7
    /// H264 only
    pub h264_packet_type: Option<VideoTagH264PacketType>,
    /// CompositionTime 24bits IF CodecID == 7
    /// H264 only
    pub composition_time: Option<i32>,

    pub data: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoTagFrameType {
    Keyframe,
    Interframe,
    DisposableInterframe, // H263 only
    GeneratedKeyframe,
    VideoInfoOrCommandFrame,
}

impl VideoTagFrameType {
    pub(crate) fn from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
        match value {
            1 => Ok(Self::Keyframe),
            2 => Ok(Self::Interframe),
            3 => Ok(Self::DisposableInterframe),
            4 => Ok(Self::GeneratedKeyframe),
            5 => Ok(Self::VideoInfoOrCommandFrame),
            _ => Err(FlvVideoTagParseError::UnknownFrameType(value)),
        }
    }

    pub(crate) fn into_raw(self) -> u8 {
        match self {
            VideoTagFrameType::Keyframe => 1,
            VideoTagFrameType::Interframe => 2,
            VideoTagFrameType::DisposableInterframe => 3,
            VideoTagFrameType::GeneratedKeyframe => 4,
            VideoTagFrameType::VideoInfoOrCommandFrame => 5,
        }
    }
}

/// FLV legacy video codec id (4-bit CodecID on the wire).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LegacyFlvVideoCodec {
    SorensonH263,
    ScreenVideo,
    Vp6,
    Vp6WithAlpha,
    ScreenVideo2,
    H264,
}

impl LegacyFlvVideoCodec {
    fn try_from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
        match value {
            2 => Ok(Self::SorensonH263),
            3 => Ok(Self::ScreenVideo),
            4 => Ok(Self::Vp6),
            5 => Ok(Self::Vp6WithAlpha),
            6 => Ok(Self::ScreenVideo2),
            7 => Ok(Self::H264),
            _ => Err(FlvVideoTagParseError::UnknownCodecId(value)),
        }
    }

    fn into_raw(self) -> u8 {
        match self {
            Self::SorensonH263 => 2,
            Self::ScreenVideo => 3,
            Self::Vp6 => 4,
            Self::Vp6WithAlpha => 5,
            Self::ScreenVideo2 => 6,
            Self::H264 => 7,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoTagH264PacketType {
    Config,
    Data,
    Eos,
}

impl VideoTagH264PacketType {
    fn from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
        match value {
            0 => Ok(Self::Config),
            1 => Ok(Self::Data),
            2 => Ok(Self::Eos),
            _ => Err(FlvVideoTagParseError::InvalidAvcPacketType(value)),
        }
    }

    fn into_raw(self) -> u8 {
        match self {
            Self::Config => 0,
            Self::Data => 1,
            Self::Eos => 2,
        }
    }
}

/// Parses SI24 (signed 24-bit integer) composition time from 3 bytes.
/// Negative values occur with B-frames where PTS < DTS.
pub(super) fn parse_composition_time(bytes: &[u8]) -> i32 {
    // bytes are placed at the top of i32 so that the SI24 sign bit aligns
    // with the i32 sign bit, arithmetic right-shift then propagates it
    i32::from_be_bytes([bytes[0], bytes[1], bytes[2], 0]) >> 8
}

/// Serializes SI24 composition time to 3 bytes.
pub(super) fn serialize_composition_time(buf: &mut BytesMut, ct: i32) {
    buf.put(&ct.to_be_bytes()[1..4]);
}

// Currently only AVC video codec is supported
impl VideoTag {
    /// Parses flv `VIDEODATA`. The `data` must be the entire content of the `Data` field of
    /// the flv tag with video `TagType`.  
    /// Check <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=74> for more info.
    pub(super) fn parse(data: Bytes) -> Result<Self, FlvVideoTagParseError> {
        if data.is_empty() {
            return Err(FlvVideoTagParseError::TooShort);
        }

        let frame_type = (data[0] & 0b11110000) >> 4;
        let codec_id = data[0] & 0b00001111;

        let frame_type = VideoTagFrameType::from_raw(frame_type)?;
        let codec = LegacyFlvVideoCodec::try_from_raw(codec_id)?;
        match codec {
            LegacyFlvVideoCodec::H264 => Self::parse_h264(data, frame_type),
            _ => Ok(Self {
                h264_packet_type: None,
                composition_time: None,
                codec,
                frame_type,
                data: data.slice(1..),
            }),
        }
    }

    fn parse_h264(
        data: Bytes,
        frame_type: VideoTagFrameType,
    ) -> Result<Self, FlvVideoTagParseError> {
        if data.len() < 5 {
            return Err(FlvVideoTagParseError::TooShort);
        }
        let avc_packet_type = VideoTagH264PacketType::from_raw(data[1])?;
        let composition_time = parse_composition_time(&data[2..5]);

        Ok(Self {
            frame_type,
            codec: LegacyFlvVideoCodec::H264,
            h264_packet_type: Some(avc_packet_type),
            composition_time: Some(composition_time),
            data: data.slice(5..),
        })
    }

    pub(super) fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        let frame_type = self.frame_type.into_raw();
        let codec_id = self.codec.into_raw();

        let first_byte = (frame_type << 4) | codec_id;
        match self.codec {
            LegacyFlvVideoCodec::H264 => self.serialize_h264(first_byte),
            _ => {
                let mut data = BytesMut::with_capacity(self.data.len() + 1);
                data.put_u8(first_byte);
                data.put(&self.data[..]);
                Ok(data.freeze())
            }
        }
    }

    fn serialize_h264(&self, first_byte: u8) -> Result<Bytes, RtmpMessageSerializeError> {
        let mut data = BytesMut::with_capacity(self.data.len() + 5);
        data.put_u8(first_byte);
        let Some(packet_type) = self.h264_packet_type else {
            return Err(RtmpMessageSerializeError::InternalError(
                "Packet type is required for H264".into(),
            ));
        };
        data.put_u8(packet_type.into_raw());
        // composition_time is 0 when packet type is config
        serialize_composition_time(&mut data, self.composition_time.unwrap_or(0));
        data.put(&self.data[..]);
        Ok(data.freeze())
    }
}
