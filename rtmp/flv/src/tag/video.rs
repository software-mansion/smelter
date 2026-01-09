use bytes::Bytes;

use crate::{
    error::{ParseError, VideoTagParseError},
    tag::PacketType,
};

/// Struct representing flv VIDEODATA.
#[derive(Debug, Clone)]
pub struct VideoTag {
    pub packet_type: PacketType,
    pub codec: VideoCodec,

    /// This field is `Some` only for tag containing AVC config.
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
pub enum FrameType {
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

        let frame_type = (data[0] >> 4) & 0x0F;
        let codec_id = data[0] & 0x0F;

        let frame_type = match frame_type {
            1 => FrameType::Keyframe,
            2 => FrameType::Interframe,
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

    fn parse_h264(mut data: Bytes, frame_type: FrameType) -> Result<Self, ParseError> {
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
        _frame_type: FrameType,
    ) -> Result<Self, ParseError> {
        Err(ParseError::UnsupportedCodec(codec.into_id()))
    }
}
