use bytes::{BufMut, Bytes, BytesMut};

use crate::{RtmpMessageSerializeError, error::FlvVideoTagParseError};

/// FourCC video codec identifiers for Enhanced RTMP.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoFourCc {
    /// H.264/AVC (`avc1`)
    Avc1,
    /// H.265/HEVC (`hvc1`)
    Hvc1,
    /// VP9 (`vp09`)
    Vp09,
    /// AV1 (`av01`)
    Av01,
}

impl VideoFourCc {
    fn from_bytes(bytes: [u8; 4]) -> Result<Self, FlvVideoTagParseError> {
        match &bytes {
            b"avc1" => Ok(Self::Avc1),
            b"hvc1" => Ok(Self::Hvc1),
            b"vp09" => Ok(Self::Vp09),
            b"av01" => Ok(Self::Av01),
            _ => Err(FlvVideoTagParseError::UnknownVideoFourCc(bytes)),
        }
    }

    fn to_bytes(self) -> [u8; 4] {
        match self {
            Self::Avc1 => *b"avc1",
            Self::Hvc1 => *b"hvc1",
            Self::Vp09 => *b"vp09",
            Self::Av01 => *b"av01",
        }
    }
}

/// Enhanced RTMP video packet types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExVideoPacketType {
    /// Decoder configuration record (SPS/PPS, VPS, etc.)
    SequenceStart,
    /// Video frame data with SI24 composition time offset.
    CodedFrames,
    /// End of sequence marker.
    SequenceEnd,
    /// Video frame data without composition time (implied 0).
    CodedFramesX,
    /// AMF-encoded metadata (e.g. HDR colorInfo).
    Metadata,
}

impl ExVideoPacketType {
    fn from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
        match value {
            0 => Ok(Self::SequenceStart),
            1 => Ok(Self::CodedFrames),
            2 => Ok(Self::SequenceEnd),
            3 => Ok(Self::CodedFramesX),
            4 => Ok(Self::Metadata),
            _ => Err(FlvVideoTagParseError::UnknownExVideoPacketType(value)),
        }
    }

    fn into_raw(self) -> u8 {
        match self {
            Self::SequenceStart => 0,
            Self::CodedFrames => 1,
            Self::SequenceEnd => 2,
            Self::CodedFramesX => 3,
            Self::Metadata => 4,
        }
    }
}

/// Struct representing flv VIDEODATA.
#[derive(Debug, Clone)]
pub enum VideoTag {
    Legacy(LegacyVideoTag),
    Enhanced(EnhancedVideoTag),
}

/// Legacy (non-enhanced) video tag.
#[derive(Debug, Clone)]
pub struct LegacyVideoTag {
    /// FrameType 4bits
    pub frame_type: VideoTagFrameType,
    /// CodecID 4bits
    pub codec: VideoCodec,

    /// AVCPacketType 8bits IF CodecID == 7
    /// H264 only
    pub h264_packet_type: Option<VideoTagH264PacketType>,
    /// CompositionTime 24bits IF CodecID == 7
    /// H264 only
    pub composition_time: Option<i32>,

    pub data: Bytes,
}

/// Enhanced RTMP video tag (E-RTMP v1+).
#[derive(Debug, Clone)]
pub struct EnhancedVideoTag {
    /// FrameType 3bits (bits 6-4 of byte 0)
    pub frame_type: VideoTagFrameType,
    /// FourCC codec identifier
    pub fourcc: VideoFourCc,
    /// Enhanced video packet type
    pub packet_type: ExVideoPacketType,
    /// Composition time offset (only for CodedFrames packet type)
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
    fn from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
        match value {
            1 => Ok(Self::Keyframe),
            2 => Ok(Self::Interframe),
            3 => Ok(Self::DisposableInterframe),
            4 => Ok(Self::GeneratedKeyframe),
            5 => Ok(Self::VideoInfoOrCommandFrame),
            _ => Err(FlvVideoTagParseError::UnknownFrameType(value)),
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

impl VideoTag {
    /// Parses flv `VIDEODATA`. The `data` must be the entire content of the `Data` field of
    /// the flv tag with video `TagType`.
    /// Check <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=74> for more info.
    /// Enhanced RTMP: <https://veovera.org/docs/enhanced/enhanced-rtmp-v1>
    pub fn parse(data: Bytes) -> Result<Self, FlvVideoTagParseError> {
        if data.is_empty() {
            return Err(FlvVideoTagParseError::TooShort);
        }

        let is_ex_header = (data[0] & 0b10000000) != 0;

        if is_ex_header {
            Self::parse_enhanced(data)
        } else {
            Self::parse_legacy(data)
        }
    }

    fn parse_legacy(data: Bytes) -> Result<Self, FlvVideoTagParseError> {
        let frame_type = (data[0] & 0b11110000) >> 4;
        let codec_id = data[0] & 0b00001111;

        let frame_type = VideoTagFrameType::from_raw(frame_type)?;
        let codec = VideoCodec::try_from_raw(codec_id)?;
        match codec {
            VideoCodec::H264 => Ok(Self::Legacy(LegacyVideoTag::parse_h264(data, frame_type)?)),
            _ => Ok(Self::Legacy(LegacyVideoTag {
                h264_packet_type: None,
                composition_time: None,
                codec,
                frame_type,
                data: data.slice(1..),
            })),
        }
    }

    fn parse_enhanced(data: Bytes) -> Result<Self, FlvVideoTagParseError> {
        // Byte 0: [IsExHeader(1) | FrameType(3) | VideoPacketType(4)]
        let frame_type_raw = (data[0] & 0b01110000) >> 4;
        let packet_type_raw = data[0] & 0b00001111;

        let frame_type = VideoTagFrameType::from_raw(frame_type_raw)?;
        let packet_type = ExVideoPacketType::from_raw(packet_type_raw)?;

        // FourCC follows byte 0 (4 bytes)
        if data.len() < 5 {
            return Err(FlvVideoTagParseError::TooShort);
        }

        let fourcc_bytes: [u8; 4] = [data[1], data[2], data[3], data[4]];
        let fourcc = VideoFourCc::from_bytes(fourcc_bytes)?;

        match packet_type {
            ExVideoPacketType::CodedFrames => {
                // SI24 composition time + frame data
                if data.len() < 8 {
                    return Err(FlvVideoTagParseError::TooShort);
                }
                let composition_time = i32::from_be_bytes([0, data[5], data[6], data[7]]);
                Ok(Self::Enhanced(EnhancedVideoTag {
                    frame_type,
                    fourcc,
                    packet_type,
                    composition_time: Some(composition_time),
                    data: data.slice(8..),
                }))
            }
            ExVideoPacketType::CodedFramesX => {
                // No composition time, frame data starts immediately after FourCC
                Ok(Self::Enhanced(EnhancedVideoTag {
                    frame_type,
                    fourcc,
                    packet_type,
                    composition_time: None,
                    data: data.slice(5..),
                }))
            }
            ExVideoPacketType::SequenceStart | ExVideoPacketType::Metadata => {
                // Config data / metadata after FourCC
                Ok(Self::Enhanced(EnhancedVideoTag {
                    frame_type,
                    fourcc,
                    packet_type,
                    composition_time: None,
                    data: data.slice(5..),
                }))
            }
            ExVideoPacketType::SequenceEnd => {
                // No payload after FourCC
                Ok(Self::Enhanced(EnhancedVideoTag {
                    frame_type,
                    fourcc,
                    packet_type,
                    composition_time: None,
                    data: Bytes::new(),
                }))
            }
        }
    }

    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        match self {
            Self::Legacy(tag) => tag.serialize(),
            Self::Enhanced(tag) => tag.serialize(),
        }
    }
}

impl LegacyVideoTag {
    fn parse_h264(
        data: Bytes,
        frame_type: VideoTagFrameType,
    ) -> Result<Self, FlvVideoTagParseError> {
        if data.len() < 5 {
            return Err(FlvVideoTagParseError::TooShort);
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

    fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        let frame_type = self.frame_type.into_raw();
        let codec_id = self.codec.into_raw();

        let first_byte = (frame_type << 4) | codec_id;
        match self.codec {
            VideoCodec::H264 => self.serialize_h264(first_byte),
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
        data.put(&self.composition_time.unwrap_or(0).to_be_bytes()[1..4]);
        data.put(&self.data[..]);
        Ok(data.freeze())
    }
}

impl EnhancedVideoTag {
    fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        // Byte 0: [IsExHeader(1) | FrameType(3) | VideoPacketType(4)]
        let first_byte =
            0b10000000 | (self.frame_type.into_raw() << 4) | self.packet_type.into_raw();
        let fourcc = self.fourcc.to_bytes();

        let extra_size = match self.packet_type {
            ExVideoPacketType::CodedFrames => 3, // SI24 composition time
            _ => 0,
        };

        let mut buf = BytesMut::with_capacity(1 + 4 + extra_size + self.data.len());
        buf.put_u8(first_byte);
        buf.put(&fourcc[..]);

        if self.packet_type == ExVideoPacketType::CodedFrames {
            let ct = self.composition_time.unwrap_or(0);
            buf.put(&ct.to_be_bytes()[1..4]);
        }

        buf.put(&self.data[..]);
        Ok(buf.freeze())
    }
}
