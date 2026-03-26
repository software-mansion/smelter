use bytes::{BufMut, Bytes, BytesMut};

use crate::{RtmpMessageSerializeError, error::FlvVideoTagParseError};

use super::ex_video::ExVideoPacketType;

/// Enhanced RTMP ModEx sub-type for video packets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum VideoPacketModExType {
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

    fn into_raw(self) -> u8 {
        match self {
            Self::TimestampOffsetNano => 0,
        }
    }
}

/// Result of resolving ModEx prefixes from the wire.
pub(super) struct ModExResult {
    pub packet_type: ExVideoPacketType,
    pub remaining: Bytes,
    pub timestamp_offset_nanos: Option<u32>,
}

/// Processes the ModEx prefix loop, returning the resolved packet type,
/// remaining data, and any collected modifiers (e.g. nanosecond timestamp offset).
///
/// Each ModEx iteration:
/// 1. UI8 + 1 data size (if 256, use UI16 + 1)
/// 2. ModEx data payload
/// 3. `[VideoPacketModExType(4 bits) | ExVideoPacketType(4 bits)]`
/// 4. Interpret data based on ModExType, then check if PacketType is another ModEx.
pub(super) fn resolve_mod_ex(data: Bytes) -> Result<ModExResult, FlvVideoTagParseError> {
    let mut offset: usize = 0;
    let mut timestamp_offset_nanos: Option<u32> = None;

    loop {
        // Read ModEx data size: UI8 + 1 (range 1..=256)
        if data.len() < offset + 1 {
            return Err(FlvVideoTagParseError::TooShort);
        }
        let mut mod_ex_data_size = data[offset] as usize + 1;
        offset += 1;

        if mod_ex_data_size == 256 {
            if data.len() < offset + 2 {
                return Err(FlvVideoTagParseError::TooShort);
            }
            mod_ex_data_size = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize + 1;
            offset += 2;
        }

        if data.len() < offset + mod_ex_data_size {
            return Err(FlvVideoTagParseError::TooShort);
        }
        let mod_ex_data_start = offset;
        offset += mod_ex_data_size;

        // Next byte: [VideoPacketModExType(4 bits) | ExVideoPacketType(4 bits)]
        if data.len() < offset + 1 {
            return Err(FlvVideoTagParseError::TooShort);
        }
        let mod_ex_type = VideoPacketModExType::from_raw((data[offset] & 0b11110000) >> 4)?;
        let next_packet_type = ExVideoPacketType::from_raw(data[offset] & 0b00001111)?;
        offset += 1;

        match mod_ex_type {
            VideoPacketModExType::TimestampOffsetNano => {
                let mod_ex_data = &data[mod_ex_data_start..mod_ex_data_start + mod_ex_data_size];
                if mod_ex_data.len() >= 3 {
                    timestamp_offset_nanos = Some(u32::from_be_bytes([
                        0,
                        mod_ex_data[0],
                        mod_ex_data[1],
                        mod_ex_data[2],
                    ]));
                }
            }
        }

        if next_packet_type != ExVideoPacketType::ModEx {
            return Ok(ModExResult {
                packet_type: next_packet_type,
                remaining: data.slice(offset..),
                timestamp_offset_nanos,
            });
        }
    }
}

/// Serializes a single ModEx entry into `buf`.
///
/// Wire format:
/// 1. UI8 data_size - 1 (or 0xFF followed by UI16 data_size - 1 if size >= 256)
/// 2. ModEx data payload
/// 3. `[VideoPacketModExType(4 bits) | ExVideoPacketType(4 bits)]`
pub(super) fn serialize_mod_ex(
    buf: &mut BytesMut,
    mod_ex_type: VideoPacketModExType,
    mod_ex_data: &[u8],
    next_packet_type: ExVideoPacketType,
) -> Result<(), RtmpMessageSerializeError> {
    let data_size = mod_ex_data.len();

    if data_size == 0 {
        return Err(RtmpMessageSerializeError::InternalError(
            "ModEx data must be at least 1 byte".into(),
        ));
    }

    if data_size >= 256 {
        buf.put_u8(0xFF);
        buf.put_u16((data_size - 1) as u16);
    } else {
        buf.put_u8((data_size - 1) as u8);
    }

    buf.put(mod_ex_data);

    let type_byte = (mod_ex_type.into_raw() << 4) | next_packet_type.into_raw();
    buf.put_u8(type_byte);
    Ok(())
}
