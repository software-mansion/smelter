use bytes::Bytes;
use thiserror::Error;

use crate::{FrameType, ParseError, VideoCodec, VideoTag, tag::PacketType};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum VideoTagParseError {
    #[error("Invalid AvcPacketType header value: {0}")]
    InvalidAvcPacketType(u8),

    #[error("Unsupported frame type header value: {0}")]
    UnsupportedFrameType(u8),
}

impl TryFrom<u8> for VideoCodec {
    type Error = ParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use crate::VideoCodec::*;
        match value {
            2 => Ok(SorensonH263),
            3 => Ok(ScreenVideo),
            4 => Ok(Vp6),
            5 => Ok(Vp6WithAlpha),
            6 => Ok(ScreenVideo2),
            7 => Ok(H264),
            _ => Err(ParseError::UnsupportedCodec(value)),
        }
    }
}

impl From<VideoCodec> for u8 {
    fn from(value: VideoCodec) -> Self {
        use crate::VideoCodec::*;
        match value {
            SorensonH263 => 2,
            ScreenVideo => 3,
            Vp6 => 4,
            Vp6WithAlpha => 5,
            ScreenVideo2 => 6,
            H264 => 7,
        }
    }
}

impl VideoTag {
    pub(super) fn parse(data: Bytes) -> Result<Self, ParseError> {
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

        let codec = VideoCodec::try_from(codec_id)?;
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
        Err(ParseError::UnsupportedCodec(codec.into()))
    }
}
