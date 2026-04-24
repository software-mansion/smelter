use bytes::Bytes;

use crate::error::FlvAudioTagParseError;

use super::ex_audio::ExAudioPacketType;

/// Enhanced RTMP ModEx sub-type for audio packets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum AudioPacketModExType {
    /// Nanosecond precision timestamp offset (UI24, max 999,999 ns).
    TimestampOffsetNano,
}

impl AudioPacketModExType {
    fn from_raw(value: u8) -> Result<Self, FlvAudioTagParseError> {
        match value {
            0 => Ok(Self::TimestampOffsetNano),
            _ => Err(FlvAudioTagParseError::UnknownAudioPacketModExType(value)),
        }
    }
}

const MAX_TIMESTAMP_OFFSET_NANOS: u32 = 999_999;

/// Result of resolving ModEx prefixes from the wire.
pub(super) struct ModExResult {
    pub packet_type: ExAudioPacketType,
    pub remaining: Bytes,
    pub timestamp_offset_nanos: Option<u32>,
}

/// Processes the ModEx prefix loop, returning the resolved packet type,
/// remaining data, and any collected modifiers (e.g. nanosecond timestamp offset).
///
/// Each ModEx iteration:
/// 1. UI8 + 1 data size (if 256, use UI16 + 1)
/// 2. ModEx data payload
/// 3. `[AudioPacketModExType(4 bits) | ExAudioPacketType(4 bits)]`
/// 4. Interpret data based on ModExType, then check if PacketType is another ModEx.
pub(super) fn resolve_mod_ex(data: Bytes) -> Result<ModExResult, FlvAudioTagParseError> {
    let mut offset: usize = 0;
    let mut timestamp_offset_nanos: Option<u32> = None;

    loop {
        // Read ModEx data size: UI8 + 1 (range 1..=256)
        if data.len() < offset + 1 {
            return Err(FlvAudioTagParseError::TooShort);
        }
        let mut mod_ex_data_size = data[offset] as usize + 1;
        offset += 1;

        if mod_ex_data_size == 256 {
            if data.len() < offset + 2 {
                return Err(FlvAudioTagParseError::TooShort);
            }
            mod_ex_data_size = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize + 1;
            offset += 2;
        }

        if data.len() < offset + mod_ex_data_size {
            return Err(FlvAudioTagParseError::TooShort);
        }
        let mod_ex_data_start = offset;
        offset += mod_ex_data_size;

        // Next byte: [AudioPacketModExType(4 bits) | ExAudioPacketType(4 bits)]
        if data.len() < offset + 1 {
            return Err(FlvAudioTagParseError::TooShort);
        }
        let mod_ex_type = AudioPacketModExType::from_raw((data[offset] & 0b11110000) >> 4)?;
        let next_packet_type = ExAudioPacketType::from_raw(data[offset] & 0b00001111)?;
        offset += 1;

        match mod_ex_type {
            AudioPacketModExType::TimestampOffsetNano => {
                let mod_ex_data = &data[mod_ex_data_start..mod_ex_data_start + mod_ex_data_size];
                if mod_ex_data.len() < 3 {
                    return Err(FlvAudioTagParseError::TooShort);
                }

                let nanos = u32::from_be_bytes([0, mod_ex_data[0], mod_ex_data[1], mod_ex_data[2]]);
                if nanos > MAX_TIMESTAMP_OFFSET_NANOS {
                    return Err(FlvAudioTagParseError::InvalidTimestampOffsetNanos(nanos));
                }

                let _ = timestamp_offset_nanos.replace(nanos);
            }
        }

        if next_packet_type != ExAudioPacketType::ModEx {
            return Ok(ModExResult {
                packet_type: next_packet_type,
                remaining: data.slice(offset..),
                timestamp_offset_nanos,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{ExAudioPacketType, resolve_mod_ex};
    use crate::error::FlvAudioTagParseError;

    #[test]
    fn resolve_mod_ex_rejects_short_timestamp_offset_payload() {
        // data size = 2 bytes (encoded as UI8 value 1, then +1)
        let data = Bytes::from_static(&[
            1, 0xAA, 0xBB,
            0x00, // [modExType=0 TimestampOffsetNano | nextPacketType=0 SequenceStart]
        ]);

        let result = resolve_mod_ex(data);
        assert!(matches!(result, Err(FlvAudioTagParseError::TooShort)));
    }

    #[test]
    fn resolve_mod_ex_rejects_timestamp_offset_above_spec_max() {
        // 1_000_000 ns (0x0F4240) exceeds the v2 spec max of 999_999 ns.
        let data = Bytes::from_static(&[
            2, 0x0F, 0x42, 0x40,
            0x01, // [modExType=0 TimestampOffsetNano | nextPacketType=1 CodedFrames]
        ]);

        let result = resolve_mod_ex(data);
        assert!(matches!(
            result,
            Err(FlvAudioTagParseError::InvalidTimestampOffsetNanos(
                1_000_000
            ))
        ));
    }

    #[test]
    fn resolve_mod_ex_accepts_valid_timestamp_offset() {
        // 999_999 ns (0x0F423F)
        let data = Bytes::from_static(&[
            2, 0x0F, 0x42, 0x3F,
            0x01, // [modExType=0 TimestampOffsetNano | nextPacketType=1 CodedFrames]
        ]);

        let result = resolve_mod_ex(data).unwrap();
        assert_eq!(result.packet_type, ExAudioPacketType::CodedFrames);
        assert_eq!(result.timestamp_offset_nanos, Some(999_999));
        assert!(result.remaining.is_empty());
    }
}
