use bytes::{BufMut, Bytes, BytesMut};

use crate::{RtmpMessageSerializeError, error::FlvVideoTagParseError};

use super::{
    EX_HEADER_BIT,
    mod_ex::{VideoPacketModExType, resolve_mod_ex, serialize_mod_ex},
    video::{VideoTagFrameType, parse_composition_time, serialize_composition_time},
};

/// Parsed Enhanced RTMP video tag.
#[derive(Debug, Clone, PartialEq)]
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

#[allow(unused)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// H.266/VVC (`vvc1`)
    Vvc1,
}

impl ExVideoFourCc {
    fn from_raw(bytes: [u8; 4]) -> Result<Self, FlvVideoTagParseError> {
        match &bytes {
            b"vp08" => Ok(Self::Vp08),
            b"vp09" => Ok(Self::Vp09),
            b"av01" => Ok(Self::Av01),
            b"avc1" => Ok(Self::Avc1),
            b"hvc1" => Ok(Self::Hvc1),
            b"vvc1" => Ok(Self::Vvc1),
            _ => Err(FlvVideoTagParseError::UnknownVideoFourCc(bytes)),
        }
    }

    fn into_raw(self) -> [u8; 4] {
        match self {
            Self::Vp08 => *b"vp08",
            Self::Vp09 => *b"vp09",
            Self::Av01 => *b"av01",
            Self::Avc1 => *b"avc1",
            Self::Hvc1 => *b"hvc1",
            Self::Vvc1 => *b"vvc1",
        }
    }

    fn has_composition_time(self) -> bool {
        matches!(self, Self::Avc1 | Self::Hvc1 | Self::Vvc1)
    }
}

/// Semantic video packet type after parsing.
///
/// This represents the resolved body content. Wire-only signals (`ModEx`, `Multitrack`)
/// are handled during parsing and do not appear here. `CodedFrames` and `CodedFramesX`
/// from the wire are unified — `CodedFramesX` sets `composition_time = 0`.
#[derive(Debug, Clone, PartialEq)]
pub enum ExVideoPacket {
    /// Decoder configuration record (SPS/PPS, VPS, etc.)
    SequenceStart(Bytes),
    /// Video frame data with composition time offset.
    /// For codecs without composition time (VP8, VP9, AV1), `composition_time` is 0.
    /// Encompasses both wire types `CodedFrames` (explicit composition time) and `CodedFramesX` (implicit 0).
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

    pub(super) fn into_raw(self) -> u8 {
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
        // Note: any ModEx modifiers (e.g. timestamp_offset_nanos) that preceded
        // this command are intentionally discarded — they are not meaningful for
        // seek signaling.
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

    /// Parses CodedFrames body. AVC, HEVC, and VVC include a 3-byte signed
    /// composition time offset prefix; other codecs do not (composition_time
    /// is set to 0 in the parsed representation).
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

    pub(super) fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        match self {
            ExVideoTag::StartSeek | ExVideoTag::EndSeek => {
                let command_byte = match self {
                    ExVideoTag::StartSeek => 0u8,
                    ExVideoTag::EndSeek => 1u8,
                    _ => unreachable!(),
                };
                // Spec requires VideoInfoOrCommandFrame + any non-Metadata packet type
                // for commands. We use SequenceStart (0) as a convention.
                let first_byte = EX_HEADER_BIT
                    | (VideoTagFrameType::VideoInfoOrCommandFrame.into_raw() << 4)
                    | ExVideoPacketType::SequenceStart.into_raw();
                let mut data = BytesMut::with_capacity(2);
                data.put_u8(first_byte);
                data.put_u8(command_byte);
                Ok(data.freeze())
            }
            ExVideoTag::VideoBody {
                four_cc,
                packet,
                frame_type,
                timestamp_offset_nanos,
            } => {
                let (wire_packet_type, needs_composition_time) = match packet {
                    ExVideoPacket::SequenceStart(_) => (ExVideoPacketType::SequenceStart, false),
                    ExVideoPacket::CodedFrames {
                        composition_time, ..
                    } => {
                        if !four_cc.has_composition_time() {
                            // VP8/VP9/AV1: no composition time on wire
                            (ExVideoPacketType::CodedFrames, false)
                        } else if *composition_time != 0 {
                            // AVC/HEVC/VVC with nonzero composition time
                            (ExVideoPacketType::CodedFrames, true)
                        } else {
                            // AVC/HEVC/VVC with zero composition time: use CodedFramesX
                            // to skip the 3-byte composition time field on wire
                            (ExVideoPacketType::CodedFramesX, false)
                        }
                    }
                    ExVideoPacket::SequenceEnd => (ExVideoPacketType::SequenceEnd, false),
                    ExVideoPacket::Metadata(_) => (ExVideoPacketType::Metadata, false),
                    ExVideoPacket::Mpeg2TsSequenceStart(_) => {
                        (ExVideoPacketType::Mpeg2TsSequenceStart, false)
                    }
                };

                let has_mod_ex = timestamp_offset_nanos.is_some();
                let header_packet_type = if has_mod_ex {
                    ExVideoPacketType::ModEx
                } else {
                    wire_packet_type
                };

                let first_byte =
                    EX_HEADER_BIT | (frame_type.into_raw() << 4) | header_packet_type.into_raw();

                let body_data = match packet {
                    ExVideoPacket::SequenceStart(data) => &data[..],
                    ExVideoPacket::CodedFrames { data, .. } => &data[..],
                    ExVideoPacket::SequenceEnd => &[][..],
                    ExVideoPacket::Metadata(data) => &data[..],
                    ExVideoPacket::Mpeg2TsSequenceStart(data) => &data[..],
                };

                // TimestampOffsetNano payload is UI24 (3 bytes), max 999,999 ns per spec.
                let mod_ex_data: Option<[u8; 3]> = match timestamp_offset_nanos {
                    Some(nanos) if *nanos > 999_999 => {
                        return Err(RtmpMessageSerializeError::InternalError(format!(
                            "timestamp_offset_nanos {nanos} exceeds max 999999"
                        )));
                    }
                    Some(nanos) => {
                        let bytes = nanos.to_be_bytes();
                        Some([bytes[1], bytes[2], bytes[3]])
                    }
                    None => None,
                };
                // ModEx wire overhead: 1 (size) + payload + 1 (type byte)
                let mod_ex_size = mod_ex_data.as_ref().map_or(0, |data| data.len() + 2);
                let composition_time_size = if needs_composition_time { 3 } else { 0 };
                let capacity = 1 + mod_ex_size + 4 + composition_time_size + body_data.len();

                let mut buf = BytesMut::with_capacity(capacity);
                buf.put_u8(first_byte);

                if let Some(data) = &mod_ex_data {
                    serialize_mod_ex(
                        &mut buf,
                        VideoPacketModExType::TimestampOffsetNano,
                        data,
                        wire_packet_type,
                    )?;
                }

                buf.put(&four_cc.into_raw()[..]);

                if needs_composition_time
                    && let ExVideoPacket::CodedFrames {
                        composition_time, ..
                    } = packet
                {
                    serialize_composition_time(&mut buf, *composition_time);
                }

                buf.put(body_data);
                Ok(buf.freeze())
            }
        }
    }
}
