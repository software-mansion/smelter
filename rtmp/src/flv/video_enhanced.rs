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
}

/// Struct representing Enhanced RTMP video data (ExVideoHeader).
#[derive(Debug, Clone)]
pub struct EnhancedVideoTag {
    /// FrameType from lower 4 bits of the first byte.
    pub frame_type: VideoTagFrameType,
    /// The resolved VideoPacketType (after processing any ModEx prefixes).
    pub packet_type: VideoPacketType,
    /// FourCC codec identifier (4 bytes after the first byte).
    pub four_cc: VideoFourCc,
    /// CompositionTime SI24, present only for CodedFrames packet type.
    pub composition_time: Option<i32>,
    /// Nanosecond timestamp offset from ModEx, if present.
    pub timestamp_nano_offset: Option<u32>,

    pub data: Bytes,
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

/// Enhanced RTMP video packet types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoPacketType {
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
    /// Carriage of bitstream in MPEG-2 TS format.
    /// Mutually exclusive with SequenceStart.
    Mpeg2TsSequenceStart,
    /// Turns on video multitrack mode.
    Multitrack,
    /// Modifier/extension signal. Carries additional modifiers (e.g. nanosecond
    /// timestamp precision) before the actual packet type.
    ModEx,
}

impl VideoPacketType {
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
            EnhancedVideoTag::parse(data).map(FlvVideoData::Enhanced)
        } else {
            VideoTag::parse(data).map(FlvVideoData::Legacy)
        }
    }

    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        match self {
            FlvVideoData::Legacy(tag) => tag.serialize(),
            FlvVideoData::Enhanced(tag) => tag.serialize(),
        }
    }
}

impl EnhancedVideoTag {
    /// Parses Enhanced RTMP video tag.
    /// First byte: `[isExHeader(1) | VideoFrameType(3 bits) | VideoPacketType(4 bits)]`
    /// Dispatches to packet-type-specific parsers.
    fn parse(data: Bytes) -> Result<Self, FlvVideoTagParseError> {
        if data.is_empty() {
            return Err(FlvVideoTagParseError::TooShort);
        }

        let frame_type = VideoTagFrameType::from_raw((data[0] & 0b01110000) >> 4)?;
        let packet_type = VideoPacketType::from_raw(data[0] & 0b00001111)?;

        match packet_type {
            VideoPacketType::CodedFrames => {
                Self::parse_coded_frames(data.slice(1..), frame_type, None)
            }
            VideoPacketType::CodedFramesX
            | VideoPacketType::SequenceStart
            | VideoPacketType::SequenceEnd
            | VideoPacketType::Mpeg2TsSequenceStart
            | VideoPacketType::Metadata => {
                Self::parse_fourcc_and_data(data.slice(1..), frame_type, packet_type, None)
            }
            VideoPacketType::ModEx => Self::parse_mod_ex(data.slice(1..), frame_type),
            VideoPacketType::Multitrack => {
                // TODO: implement multitrack parsing (AvMultitrackType + per-track FourCC/data)
                Err(FlvVideoTagParseError::UnsupportedPacketType(
                    packet_type.into_raw(),
                ))
            }
        }
    }

    /// Parses CodedFrames: FourCC (4 bytes) + optional SI24 composition time (3 bytes) + payload.
    /// Composition time is only present for AVC and HEVC codecs.
    fn parse_coded_frames(
        data: Bytes,
        frame_type: VideoTagFrameType,
        timestamp_nano_offset: Option<u32>,
    ) -> Result<Self, FlvVideoTagParseError> {
        if data.len() < 4 {
            return Err(FlvVideoTagParseError::TooShort);
        }

        let four_cc = VideoFourCc::from_bytes([data[0], data[1], data[2], data[3]])?;

        if four_cc.has_composition_time() {
            if data.len() < 7 {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let composition_time = parse_composition_time(&data[4..7]);
            Ok(Self {
                frame_type,
                packet_type: VideoPacketType::CodedFrames,
                four_cc,
                composition_time: Some(composition_time),
                timestamp_nano_offset,
                data: data.slice(7..),
            })
        } else {
            Ok(Self {
                frame_type,
                packet_type: VideoPacketType::CodedFrames,
                four_cc,
                composition_time: None,
                timestamp_nano_offset,
                data: data.slice(4..),
            })
        }
    }

    /// Parses packet types that have FourCC (4 bytes) followed by raw data
    /// (no composition time): SequenceStart, SequenceEnd, CodedFramesX,
    /// Mpeg2TsSequenceStart, Metadata.
    fn parse_fourcc_and_data(
        data: Bytes,
        frame_type: VideoTagFrameType,
        packet_type: VideoPacketType,
        timestamp_nano_offset: Option<u32>,
    ) -> Result<Self, FlvVideoTagParseError> {
        // 4 bytes FourCC
        if data.len() < 4 {
            return Err(FlvVideoTagParseError::TooShort);
        }

        let four_cc = VideoFourCc::from_bytes([data[0], data[1], data[2], data[3]])?;

        Ok(Self {
            frame_type,
            packet_type,
            four_cc,
            composition_time: None,
            timestamp_nano_offset,
            data: data.slice(4..),
        })
    }

    /// Parses ModEx prefix loop: processes modifier data packets, then
    /// delegates to the resolved packet type's parser.
    ///
    /// Each ModEx iteration:
    /// 1. UI8 + 1 data size (if 256, use UI16 + 1)
    /// 2. ModEx data payload
    /// 3. `[VideoPacketModExType(4 bits) | VideoPacketType(4 bits)]`
    /// 4. Interpret data based on ModExType, then check if PacketType is another ModEx.
    fn parse_mod_ex(
        data: Bytes,
        frame_type: VideoTagFrameType,
    ) -> Result<Self, FlvVideoTagParseError> {
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

            // Next byte: [VideoPacketModExType(4 bits) | VideoPacketType(4 bits)]
            if data.len() < offset + 1 {
                return Err(FlvVideoTagParseError::TooShort);
            }
            let mod_ex_type = VideoPacketModExType::from_raw((data[offset] & 0xF0) >> 4)?;
            let next_packet_type = VideoPacketType::from_raw(data[offset] & 0x0F)?;
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

            // If another ModEx, continue the loop; otherwise delegate to the resolved type.
            if next_packet_type != VideoPacketType::ModEx {
                let rest = data.slice(offset..);
                return match next_packet_type {
                    VideoPacketType::CodedFrames => {
                        Self::parse_coded_frames(rest, frame_type, timestamp_nano_offset)
                    }
                    VideoPacketType::Multitrack => Err(
                        FlvVideoTagParseError::UnsupportedPacketType(next_packet_type.into_raw()),
                    ),
                    _ => Self::parse_fourcc_and_data(
                        rest,
                        frame_type,
                        next_packet_type,
                        timestamp_nano_offset,
                    ),
                };
            }
        }
    }

    pub fn serialize(&self) -> Result<Bytes, RtmpMessageSerializeError> {
        unimplemented!()
    }
}
