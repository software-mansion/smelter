use bytes::{BufMut, Bytes, BytesMut};

use crate::{
    PacketType, SerializationError,
    error::{ParseError, VideoTagParseError},
};

/// Struct representing flv VIDEODATA.
#[derive(Debug, Clone)]
pub struct VideoTag {
    pub packet_type: PacketType,
    pub codec: VideoCodec,

    /// This field is `Some` only for tag containing AVC config.
    pub composition_time: Option<i32>,
    pub frame_type: VideoFrameType,
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

impl VideoCodec {
    fn try_from_id(id: u8) -> Result<Self, ParseError> {
        match id {
            2 => Ok(Self::SorensonH263),
            3 => Ok(Self::ScreenVideo),
            4 => Ok(Self::Vp6),
            5 => Ok(Self::Vp6WithAlpha),
            6 => Ok(Self::ScreenVideo2),
            7 => Ok(Self::H264),
            _ => Err(ParseError::UnsupportedCodec(id)),
        }
    }

    fn into_id(self) -> u8 {
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

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum VideoFrameType {
    #[default]
    Keyframe,
    Interframe,
}

// Currently only AVC video codec is supported
impl VideoTag {
    /// Parses flv `VIDEODATA`. The `data` must be the entire content of the `Data` field of
    /// the flv tag with video `TagType`.  
    /// Check <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=74> for more info.
    pub fn parse(data: Bytes) -> Result<Self, ParseError> {
        if data.is_empty() {
            return Err(ParseError::NotEnoughData);
        }

        let frame_type = (data[0] & 0b11110000) >> 4;
        let codec_id = data[0] & 0b00001111;

        let frame_type = match frame_type {
            1 => VideoFrameType::Keyframe,
            2 => VideoFrameType::Interframe,
            _ => {
                return Err(ParseError::Video(VideoTagParseError::UnsupportedFrameType(
                    frame_type,
                )));
            }
        };

        let codec = VideoCodec::try_from_id(codec_id)?;
        match codec {
            VideoCodec::H264 => Self::parse_h264(data, frame_type),
            _ => Self::parse_codec(data, codec, frame_type),
        }
    }

    fn parse_h264(mut data: Bytes, frame_type: VideoFrameType) -> Result<Self, ParseError> {
        if data.len() < 5 {
            return Err(ParseError::NotEnoughData);
        }
        let avc_packet_type = data[1];
        let composition_time = i32::from_be_bytes([0, data[2], data[3], data[4]]);

        let packet_type = match avc_packet_type {
            0 => PacketType::Config,
            1 => PacketType::Data,
            2 => PacketType::Data,
            _ => {
                return Err(ParseError::Video(VideoTagParseError::InvalidAvcPacketType(
                    avc_packet_type,
                )));
            }
        };

        let video_data = data.split_off(5);
        Ok(Self {
            packet_type,
            codec: VideoCodec::H264,
            composition_time: Some(composition_time),
            frame_type,
            data: video_data,
        })
    }

    // This method will be properly implemented when support for codecs different than H.264 is
    // added
    fn parse_codec(
        _data: Bytes,
        codec: VideoCodec,
        _frame_type: VideoFrameType,
    ) -> Result<Self, ParseError> {
        Err(ParseError::UnsupportedCodec(codec.into_id()))
    }

    pub fn serialize(&self) -> Result<Bytes, SerializationError> {
        let frame_type: u8 = match self.frame_type {
            VideoFrameType::Keyframe => 1,
            VideoFrameType::Interframe => 2,
        };
        let codec_id = self.codec.into_id();

        let first_byte = (frame_type << 4) | codec_id;
        match self.codec {
            VideoCodec::H264 => Ok(self.serialize_h264(first_byte)),
            _ => Err(SerializationError::UnsupportedVideoCodec(self.codec)),
        }
    }

    fn serialize_h264(&self, first_byte: u8) -> Bytes {
        let mut data = BytesMut::with_capacity(self.data.len() + 5);
        data.put_u8(first_byte);
        match self.packet_type {
            PacketType::Data => {
                data.put_u8(1);
                data.put(&self.composition_time.unwrap_or(0).to_be_bytes()[1..3]);
            }
            PacketType::Config => {
                data.put_u8(0);
                data.put(&[0, 0, 0][..]);
            }
        };
        data.put(&self.data[..]);
        data.freeze()
    }
}
