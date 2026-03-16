use bytes::Bytes;

use crate::{RtmpMessageSerializeError, error::FlvVideoTagParseError};

use super::video::{VideoTag, VideoTagFrameType, parse_composition_time};

const EX_HEADER_BIT: u8 = 0b10000000;

/// Top-level FLV video data, supporting both legacy and Enhanced RTMP formats.
///
/// Legacy format: <https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=74>
/// Enhanced RTMP: <https://veovera.org/docs/enhanced/enhanced-rtmp-v2.pdf>
#[derive(Debug, Clone)]
pub enum FlvVideoData {
    Legacy(VideoTag),
    Enhanced(EnhancedVideoTag),
    /// Enhanced RTMP command frame (e.g. seek start/end).
    /// Sent when `VideoFrameType == Command` and packet type is not Metadata.
    /// Per the spec, the payload is a single UI8 command byte with no video body.
    EnhancedCommand {
        command: VideoCommand,
        timestamp_nano_offset: Option<u32>,
    },
}

/// Video command signals for Enhanced RTMP.
/// Sent when `VideoFrameType == Command`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoCommand {
    StartSeek = 0,
    EndSeek = 1,
}

impl VideoCommand {
    fn from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
        match value {
            0 => Ok(Self::StartSeek),
            1 => Ok(Self::EndSeek),
            _ => Err(FlvVideoTagParseError::UnknownVideoCommand(value)),
        }
    }

    #[allow(unused)]
    fn into_raw(self) -> u8 {
        self as u8
    }
}

/// Parsed Enhanced RTMP video tag.
#[derive(Debug, Clone)]
pub struct EnhancedVideoTag {
    pub frame_type: VideoTagFrameType,
    pub four_cc: VideoFourCc,
    pub timestamp_nano_offset: Option<u32>,
    pub packet_type: VideoPacketType,
}

/// Semantic video packet type after parsing.
///
/// This represents the resolved body content. Wire-only signals (`ModEx`, `Multitrack`)
/// are handled during parsing and do not appear here. `CodedFrames` and `CodedFramesX`
/// from the wire are unified — `CodedFramesX` sets `composition_time = 0`.
#[derive(Debug, Clone)]
pub enum VideoPacketType {
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

/// FourCC video codec identifiers for Enhanced RTMP.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoFourCc {
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

impl VideoFourCc {
    fn from_bytes(bytes: [u8; 4]) -> Result<Self, FlvVideoTagParseError> {
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
    fn to_bytes(self) -> [u8; 4] {
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

/// Enhanced RTMP ModEx sub-type for video packets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoPacketModExType {
    /// Nanosecond precision timestamp offset (UI24, max 999,999 ns).
    TimestampOffsetNano,
}

impl VideoPacketModExType {
    fn from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
        match value {
            0 => Ok(Self::TimestampOffsetNano),
            _ => Err(FlvVideoTagParseError::UnknownVideoPacketModExType(value)),
        }
    }

    #[allow(unused)]
    fn into_raw(self) -> u8 {
        match self {
            Self::TimestampOffsetNano => 0,
        }
    }
}

/// Raw wire video packet type values (0-7). Used only during parsing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RawVideoPacketType {
    SequenceStart,
    CodedFrames,
    SequenceEnd,
    CodedFramesX,
    Metadata,
    Mpeg2TsSequenceStart,
    Multitrack,
    ModEx,
}

impl RawVideoPacketType {
    fn from_raw(value: u8) -> Result<Self, FlvVideoTagParseError> {
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

impl FlvVideoData {
    /// Parses flv `VIDEODATA`. Checks the IsExHeader bit in the first byte
    /// and dispatches to either legacy or Enhanced RTMP parsing.
    pub fn parse(data: Bytes) -> Result<Self, FlvVideoTagParseError> {
        if data.is_empty() {
            return Err(FlvVideoTagParseError::TooShort);
        }

        if data[0] & EX_HEADER_BIT != 0 {
            EnhancedVideoTag::parse(data)
        } else {
            VideoTag::parse(data).map(FlvVideoData::Legacy)
        }
    }

    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        match self {
            FlvVideoData::Legacy(tag) => tag.serialize(),
            FlvVideoData::Enhanced(tag) => tag.serialize(),
            FlvVideoData::EnhancedCommand { .. } => {
                unimplemented!()
            }
        }
    }
}

impl EnhancedVideoTag {
    /// Parses Enhanced RTMP video tag.
    /// First byte: `[isExHeader(1) | VideoFrameType(3 bits) | RawVideoPacketType(4 bits)]`
    fn parse(data: Bytes) -> Result<FlvVideoData, FlvVideoTagParseError> {
        if data.is_empty() {
            return Err(FlvVideoTagParseError::TooShort);
        }

        let frame_type = VideoTagFrameType::from_raw((data[0] & 0b01110000) >> 4)?;
        let raw_packet_type = RawVideoPacketType::from_raw(data[0] & 0b00001111)?;

        // Process ModEx to resolve the final packet type and collect modifiers.
        let (raw_packet_type, rest, timestamp_nano_offset) =
            if raw_packet_type == RawVideoPacketType::ModEx {
                Self::resolve_mod_ex(data.slice(1..))?
            } else {
                (raw_packet_type, data.slice(1..), None)
            };

        // Per spec: if frame_type is Command and packet_type is not Metadata,
        // the payload is a single UI8 VideoCommand with no video body.
        if frame_type == VideoTagFrameType::VideoInfoOrCommandFrame
            && raw_packet_type != RawVideoPacketType::Metadata
        {
            if rest.is_empty() {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let command = VideoCommand::from_raw(rest[0])?;
            return Ok(FlvVideoData::EnhancedCommand {
                command,
                timestamp_nano_offset,
            });
        }

        // Read FourCC (4 bytes), present for all non-command packet types.
        if rest.len() < 4 {
            return Err(FlvVideoTagParseError::TooShort);
        }
        let four_cc = VideoFourCc::from_bytes([rest[0], rest[1], rest[2], rest[3]])?;
        let body_data = rest.slice(4..);

        let packet_type = match raw_packet_type {
            RawVideoPacketType::SequenceStart => VideoPacketType::SequenceStart(body_data),
            RawVideoPacketType::CodedFrames => Self::parse_coded_frames(body_data, four_cc)?,
            RawVideoPacketType::CodedFramesX => VideoPacketType::CodedFrames {
                composition_time: 0,
                data: body_data,
            },
            RawVideoPacketType::SequenceEnd => VideoPacketType::SequenceEnd,
            RawVideoPacketType::Metadata => VideoPacketType::Metadata(body_data),
            RawVideoPacketType::Mpeg2TsSequenceStart => {
                VideoPacketType::Mpeg2TsSequenceStart(body_data)
            }
            RawVideoPacketType::Multitrack => {
                // TODO: implement multitrack parsing (AvMultitrackType + per-track FourCC/data)
                return Err(FlvVideoTagParseError::UnsupportedPacketType(
                    raw_packet_type.into_raw(),
                ));
            }
            RawVideoPacketType::ModEx => {
                unreachable!("ModEx should have been resolved above")
            }
        };

        Ok(FlvVideoData::Enhanced(EnhancedVideoTag {
            frame_type,
            four_cc,
            timestamp_nano_offset,
            packet_type,
        }))
    }

    /// Parses CodedFrames body: optional SI24 composition time (3 bytes) + payload.
    /// Composition time is only present for AVC and HEVC codecs; others get 0.
    fn parse_coded_frames(
        data: Bytes,
        four_cc: VideoFourCc,
    ) -> Result<VideoPacketType, FlvVideoTagParseError> {
        if four_cc.has_composition_time() {
            if data.len() < 3 {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let composition_time = parse_composition_time(&data[0..3]);
            Ok(VideoPacketType::CodedFrames {
                composition_time,
                data: data.slice(3..),
            })
        } else {
            Ok(VideoPacketType::CodedFrames {
                composition_time: 0,
                data,
            })
        }
    }

    /// Processes the ModEx prefix loop, returning the resolved raw packet type,
    /// remaining data, and any collected modifiers (e.g. nanosecond timestamp offset).
    ///
    /// Each ModEx iteration:
    /// 1. UI8 + 1 data size (if 256, use UI16 + 1)
    /// 2. ModEx data payload
    /// 3. `[VideoPacketModExType(4 bits) | RawVideoPacketType(4 bits)]`
    /// 4. Interpret data based on ModExType, then check if PacketType is another ModEx.
    fn resolve_mod_ex(
        data: Bytes,
    ) -> Result<(RawVideoPacketType, Bytes, Option<u32>), FlvVideoTagParseError> {
        let mut offset: usize = 0;
        let mut timestamp_nano_offset: Option<u32> = None;

        loop {
            // Read ModEx data size: UI8 + 1 (range 1..=256)
            if data.len() < offset + 1 {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let mut mod_ex_data_size = data[offset] as usize + 1;
            offset += 1;

            // If size == 256, use UI16 + 1 instead
            if mod_ex_data_size == 256 {
                if data.len() < offset + 2 {
                    return Err(FlvVideoTagParseError::TooShort);
                }
                mod_ex_data_size =
                    u16::from_be_bytes([data[offset], data[offset + 1]]) as usize + 1;
                offset += 2;
            }

            if data.len() < offset + mod_ex_data_size {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let mod_ex_data_start = offset;
            offset += mod_ex_data_size;

            // Next byte: [VideoPacketModExType(4 bits) | RawVideoPacketType(4 bits)]
            if data.len() < offset + 1 {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let mod_ex_type = VideoPacketModExType::from_raw((data[offset] & 0xF0) >> 4)?;
            let next_packet_type = RawVideoPacketType::from_raw(data[offset] & 0x0F)?;
            offset += 1;

            match mod_ex_type {
                VideoPacketModExType::TimestampOffsetNano => {
                    let mod_ex_data =
                        &data[mod_ex_data_start..mod_ex_data_start + mod_ex_data_size];
                    if mod_ex_data.len() >= 3 {
                        timestamp_nano_offset = Some(u32::from_be_bytes([
                            0,
                            mod_ex_data[0],
                            mod_ex_data[1],
                            mod_ex_data[2],
                        ]));
                    }
                }
            }

            // If another ModEx, continue the loop; otherwise return resolved state.
            if next_packet_type != RawVideoPacketType::ModEx {
                return Ok((
                    next_packet_type,
                    data.slice(offset..),
                    timestamp_nano_offset,
                ));
            }
        }
    }

    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        unimplemented!()
    }
}
