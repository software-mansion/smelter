use bytes::{BufMut, Bytes, BytesMut};

use crate::{
    SerializationError,
    error::{ParseError, RtmpError, VideoTagParseError},
};

/// Struct representing flv VIDEODATA.
#[derive(Debug, Clone)]
pub struct VideoTag {
    /// FrameType 4bits
    pub frame_type: VideoTagFrameType,
    /// CodecIS 4bits
    pub codec: VideoCodec,

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
    fn from_raw(value: u8) -> Result<Self, VideoTagParseError> {
        match value {
            1 => Ok(Self::Keyframe),
            2 => Ok(Self::Interframe),
            3 => Ok(Self::DisposableInterframe),
            4 => Ok(Self::GeneratedKeyframe),
            5 => Ok(Self::VideoInfoOrCommandFrame),
            _ => Err(VideoTagParseError::UnknownFrameType(value)),
        }
    }

    fn into_raw(self) -> u8 {
        match self {
            VideoTagFrameType::Keyframe => 1,
            VideoTagFrameType::Interframe => 2,
            VideoTagFrameType::DisposableInterframe => 3,
            VideoTagFrameType::GeneratedKeyframe => 4,
            VideoTagFrameType::VideoInfoOrCommandFrame => 5,
        }
    }
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

impl VideoCodec {
    fn try_from_raw(value: u8) -> Result<Self, VideoTagParseError> {
        match value {
            2 => Ok(Self::SorensonH263),
            3 => Ok(Self::ScreenVideo),
            4 => Ok(Self::Vp6),
            5 => Ok(Self::Vp6WithAlpha),
            6 => Ok(Self::ScreenVideo2),
            7 => Ok(Self::H264),
            _ => Err(VideoTagParseError::UnknownCodecId(value)),
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
    fn from_raw(value: u8) -> Result<Self, VideoTagParseError> {
        match value {
            0 => Ok(Self::Config),
            1 => Ok(Self::Data),
            2 => Ok(Self::Eos),
            _ => Err(VideoTagParseError::InvalidAvcPacketType(value)),
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

// Currently only AVC video codec is supported
impl VideoTag {
    /// Parses flv `VIDEODATA`. The `data` must be the entire content of the `Data` field of
    /// the flv tag with video `TagType`.  
    /// Check <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=74> for more info.
    pub fn parse(data: Bytes) -> Result<Self, RtmpError> {
        if data.is_empty() {
            return Err(ParseError::NotEnoughData.into());
        }

        let frame_type = (data[0] & 0b11110000) >> 4;
        let codec_id = data[0] & 0b00001111;

        let frame_type = VideoTagFrameType::from_raw(frame_type).map_err(ParseError::from)?;
        let codec = VideoCodec::try_from_raw(codec_id).map_err(ParseError::from)?;
        match codec {
            VideoCodec::H264 => Ok(Self::parse_h264(data, frame_type)?),
            _ => Ok(Self {
                h264_packet_type: None,
                composition_time: None,
                codec,
                frame_type,
                data: data.slice(1..),
            }),
        }
    }

    fn parse_h264(data: Bytes, frame_type: VideoTagFrameType) -> Result<Self, ParseError> {
        if data.len() < 5 {
            return Err(ParseError::NotEnoughData);
        }
        let avc_packet_type = VideoTagH264PacketType::from_raw(data[1])?;
        let composition_time = i32::from_be_bytes([0, data[2], data[3], data[4]]);

        Ok(Self {
            frame_type,
            codec: VideoCodec::H264,
            h264_packet_type: Some(avc_packet_type),
            composition_time: Some(composition_time),
            data: data.slice(5..),
        })
    }

    pub fn serialize(&self) -> Result<Bytes, SerializationError> {
        let frame_type = self.frame_type.into_raw();
        let codec_id = self.codec.into_raw();

        let first_byte = (frame_type << 4) | codec_id;
        match self.codec {
            VideoCodec::H264 => Ok(self.serialize_h264(first_byte)?),
            _ => {
                let mut data = BytesMut::with_capacity(self.data.len() + 1);
                data.put_u8(first_byte);
                data.put(&self.data[..]);
                Ok(data.freeze())
            }
        }
    }

    fn serialize_h264(&self, first_byte: u8) -> Result<Bytes, SerializationError> {
        let mut data = BytesMut::with_capacity(self.data.len() + 5);
        data.put_u8(first_byte);
        let Some(packet_type) = self.h264_packet_type else {
            return Err(SerializationError::H264PacketTypeRequired);
        };
        data.put_u8(packet_type.into_raw());
        // composition_time is 0 when packet type is config
        data.put(&self.composition_time.unwrap_or(0).to_be_bytes()[1..4]);
        data.put(&self.data[..]);
        Ok(data.freeze())
    }
}
