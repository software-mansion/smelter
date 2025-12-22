use bytes::Bytes;
use thiserror::Error;

use crate::{FrameType, PacketType, ParseError, VideoCodec, VideoTag};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum VideoTagParseError {
    #[error("Invalid AvcPacketType header value: {0}")]
    InvalidAvcPacketType(u8),

    #[error("Invalid frame type header value: {0}")]
    InvalidFrameType(u8),
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

impl VideoTag {
    pub fn parse(payload: &[u8]) -> Result<Self, ParseError> {
        if payload.len() < 2 {
            return Err(ParseError::NotEnoughData);
        }

        let frame_type = (payload[0] >> 4) & 0x0F;
        let codec_id = payload[0] & 0x0F;

        let codec = VideoCodec::try_from(codec_id)?;
        // NOTE: This should be removed when support for more codecs is added
        if codec != VideoCodec::H264 {
            return Err(ParseError::UnsupportedCodec(codec_id));
        } else if payload.len() < 5 {
            return Err(ParseError::NotEnoughData);
        }

        let avc_packet_type = payload[1];
        let composition_time = i32::from_be_bytes([0, payload[2], payload[3], payload[4]]);

        let packet_type = match avc_packet_type {
            0 => PacketType::VideoConfig,
            1 => PacketType::Video,
            2 => PacketType::Video,
            _ => {
                return Err(ParseError::Video(VideoTagParseError::InvalidAvcPacketType(
                    avc_packet_type,
                )));
            }
        };

        let frame_type = match frame_type {
            1 => FrameType::Keyframe,
            2 => FrameType::Interframe,
            _ => {
                return Err(ParseError::Video(VideoTagParseError::InvalidFrameType(
                    frame_type,
                )));
            }
        };

        let video_data = Bytes::copy_from_slice(&payload[5..]);
        Ok(Self {
            packet_type,
            codec,
            composition_time: Some(composition_time),
            frame_type,
            payload: video_data,
        })
    }
}
