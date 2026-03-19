use bytes::Bytes;

use crate::{RtmpMessageSerializeError, error::FlvVideoTagParseError};

use super::mod_ex::resolve_mod_ex;
use super::video::{VideoTagFrameType, parse_composition_time};

/// Parsed Enhanced RTMP video tag.
#[derive(Debug, Clone)]
pub enum ExVideoTag {
    StartSeek,
    EndSeek,
    VideoBody {
        four_cc: ExVideoFourCc,
        packet: ExVideoPacket,
        frame_type: VideoTagFrameType,
        timestamp_offset_nanos: Option<u32>,
    },
}

impl ExVideoTag {
    pub fn frame_type(&self) -> VideoTagFrameType {
        match self {
            ExVideoTag::StartSeek | ExVideoTag::EndSeek => {
                VideoTagFrameType::VideoInfoOrCommandFrame
            }
            ExVideoTag::VideoBody { frame_type, .. } => *frame_type,
        }
    }
}

/// FourCC video codec identifiers for Enhanced RTMP.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExVideoFourCc {
    /// VP8 (`vp08`)
    Vp08,
    /// VP9 (`vp09`)
    Vp09,
    /// AV1 (`av01`)
    Av01,
    /// H.264/AVC (`avc1`)
    Avc1,
    /// H.265/HEVC (`hvc1`)
    Hvc1,
}

impl ExVideoFourCc {
    fn from_raw(bytes: [u8; 4]) -> Result<Self, FlvVideoTagParseError> {
        match &bytes {
            b"vp08" => Ok(Self::Vp08),
            b"vp09" => Ok(Self::Vp09),
            b"av01" => Ok(Self::Av01),
            b"avc1" => Ok(Self::Avc1),
            b"hvc1" => Ok(Self::Hvc1),
            _ => Err(FlvVideoTagParseError::UnknownVideoFourCc(bytes)),
        }
    }

    #[allow(unused)]
    fn to_raw(self) -> [u8; 4] {
        match self {
            Self::Vp08 => *b"vp08",
            Self::Vp09 => *b"vp09",
            Self::Av01 => *b"av01",
            Self::Avc1 => *b"avc1",
            Self::Hvc1 => *b"hvc1",
        }
    }

    /// Returns true if this codec uses SI24 CompositionTime in CodedFrames.
    /// Per the spec, only AVC and HEVC carry composition time offset.
    fn has_composition_time(self) -> bool {
        matches!(self, Self::Avc1 | Self::Hvc1)
    }
}

/// Semantic video packet type after parsing.
///
/// This represents the resolved body content. Wire-only signals (`ModEx`, `Multitrack`)
/// are handled during parsing and do not appear here. `CodedFrames` and `CodedFramesX`
/// from the wire are unified — `CodedFramesX` sets `composition_time = 0`.
#[derive(Debug, Clone)]
pub enum ExVideoPacket {
    /// Decoder configuration record (SPS/PPS, VPS, etc.)
    SequenceStart(Bytes),
    /// Video frame data with composition time offset.
    /// For codecs without composition time (VP8, VP9, AV1), `composition_time` is 0.
    /// Encompasses both wire types `CodedFrames` (explicit SI24) and `CodedFramesX` (implicit 0).
    CodedFrames { composition_time: i32, data: Bytes },
    /// End of sequence marker. No payload.
    SequenceEnd,
    /// AMF-encoded metadata (e.g. HDR colorInfo).
    Metadata(Bytes),
    /// Carriage of bitstream in MPEG-2 TS format.
    /// Mutually exclusive with SequenceStart.
    Mpeg2TsSequenceStart(Bytes),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ExVideoPacketType {
    SequenceStart,
    CodedFrames,
    SequenceEnd,
    CodedFramesX,
    Metadata,
    Mpeg2TsSequenceStart,
    Multitrack,
    ModEx,
}

impl ExVideoPacketType {
    pub(super) fn from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
        match value {
            0 => Ok(Self::SequenceStart),
            1 => Ok(Self::CodedFrames),
            2 => Ok(Self::SequenceEnd),
            3 => Ok(Self::CodedFramesX),
            4 => Ok(Self::Metadata),
            5 => Ok(Self::Mpeg2TsSequenceStart),
            6 => Ok(Self::Multitrack),
            7 => Ok(Self::ModEx),
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
            Self::Mpeg2TsSequenceStart => 5,
            Self::Multitrack => 6,
            Self::ModEx => 7,
        }
    }
}

impl ExVideoTag {
    /// Parses Enhanced RTMP video tag.
    /// First byte: `[isExHeader(1) | VideoFrameType(3 bits) | VideoPacketType(4 bits)]`
    pub(super) fn parse(data: Bytes) -> Result<Self, FlvVideoTagParseError> {
        if data.is_empty() {
            return Err(FlvVideoTagParseError::TooShort);
        }

        let frame_type = VideoTagFrameType::from_raw((data[0] & 0b01110000) >> 4)?;
        let packet_type = ExVideoPacketType::from_raw(data[0] & 0b00001111)?;

        // Process ModEx to resolve the final packet type and collect modifiers.
        let (packet_type, rest, timestamp_offset_nanos) = if packet_type == ExVideoPacketType::ModEx
        {
            let result = resolve_mod_ex(data.slice(1..))?;
            (
                result.packet_type,
                result.remaining,
                result.timestamp_offset_nanos,
            )
        } else {
            (packet_type, data.slice(1..), None)
        };

        // Per spec: if frame_type is Command and packet_type is not Metadata,
        // the payload is a single UI8 VideoCommand with no FourCC or video body.
        if frame_type == VideoTagFrameType::VideoInfoOrCommandFrame
            && packet_type != ExVideoPacketType::Metadata
        {
            if rest.is_empty() {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let content = match rest[0] {
                0 => ExVideoTag::StartSeek,
                1 => ExVideoTag::EndSeek,
                v => return Err(FlvVideoTagParseError::UnknownVideoCommand(v)),
            };
            return Ok(content);
        }

        // Read FourCC (4 bytes), present for all non-command packet types.
        if rest.len() < 4 {
            return Err(FlvVideoTagParseError::TooShort);
        }
        let four_cc = ExVideoFourCc::from_raw([rest[0], rest[1], rest[2], rest[3]])?;
        let body_data = rest.slice(4..);

        let packet = match packet_type {
            ExVideoPacketType::SequenceStart => ExVideoPacket::SequenceStart(body_data),
            ExVideoPacketType::CodedFrames => Self::parse_coded_frames(body_data, four_cc)?,
            ExVideoPacketType::CodedFramesX => ExVideoPacket::CodedFrames {
                composition_time: 0,
                data: body_data,
            },
            ExVideoPacketType::SequenceEnd => ExVideoPacket::SequenceEnd,
            ExVideoPacketType::Metadata => ExVideoPacket::Metadata(body_data),
            ExVideoPacketType::Mpeg2TsSequenceStart => {
                ExVideoPacket::Mpeg2TsSequenceStart(body_data)
            }
            ExVideoPacketType::Multitrack => {
                // TODO: implement multitrack parsing (AvMultitrackType + per-track FourCC/data)
                return Err(FlvVideoTagParseError::UnsupportedPacketType(
                    packet_type.into_raw(),
                ));
            }
            ExVideoPacketType::ModEx => {
                unreachable!("ModEx should have been resolved above")
            }
        };

        Ok(ExVideoTag::VideoBody {
            four_cc,
            packet,
            frame_type,
            timestamp_offset_nanos,
        })
    }

    /// Parses CodedFrames body. AVC and HEVC include an SI24 composition
    /// time prefix; other codecs do not (composition_time is set to 0
    /// in the parsed representation).
    fn parse_coded_frames(
        data: Bytes,
        four_cc: ExVideoFourCc,
    ) -> Result<ExVideoPacket, FlvVideoTagParseError> {
        if four_cc.has_composition_time() {
            if data.len() < 3 {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let composition_time = parse_composition_time(&data[0..3]);
            Ok(ExVideoPacket::CodedFrames {
                composition_time,
                data: data.slice(3..),
            })
        } else {
            Ok(ExVideoPacket::CodedFrames {
                composition_time: 0,
                data,
            })
        }
    }

    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        unimplemented!()
    }
}
