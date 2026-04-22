//! Low-level MPEG-TS transport packet parsing.

use crate::{TS_PACKET_SIZE, TS_SYNC_BYTE, error::Error};

/// PID used by the Program Association Table.
pub const PAT_PID: u16 = 0x0000;

/// PID used by NULL/padding packets.
pub const NULL_PID: u16 = 0x1FFF;

#[derive(Debug, Clone, Copy)]
pub struct TsHeader {
    pub transport_error: bool,
    pub payload_unit_start: bool,
    pub transport_priority: bool,
    pub pid: u16,
    pub scrambling_control: u8,
    pub has_adaptation: bool,
    pub has_payload: bool,
    pub continuity_counter: u8,
}

#[derive(Debug)]
pub struct TsPacket<'a> {
    pub header: TsHeader,
    pub adaptation: &'a [u8],
    pub payload: &'a [u8],
}

impl<'a> TsPacket<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self, Error> {
        if buf.len() != TS_PACKET_SIZE {
            return Err(Error::InvalidPacketSize(buf.len()));
        }
        if buf[0] != TS_SYNC_BYTE {
            return Err(Error::InvalidSyncByte(buf[0]));
        }

        let transport_error = (buf[1] & 0x80) != 0;
        let payload_unit_start = (buf[1] & 0x40) != 0;
        let transport_priority = (buf[1] & 0x20) != 0;
        let pid = ((u16::from(buf[1]) & 0x1F) << 8) | u16::from(buf[2]);

        let scrambling_control = (buf[3] >> 6) & 0x03;
        let afc = (buf[3] >> 4) & 0x03;
        let has_adaptation = afc & 0b10 != 0;
        let has_payload = afc & 0b01 != 0;
        let continuity_counter = buf[3] & 0x0F;

        let mut offset = 4;
        let adaptation = if has_adaptation {
            let af_len = buf[offset] as usize;
            let af_start = offset + 1;
            offset = af_start + af_len;
            if offset > buf.len() {
                return Err(Error::InvalidAdaptationField);
            }
            &buf[af_start..af_start + af_len]
        } else {
            &[][..]
        };

        let payload = if has_payload { &buf[offset..] } else { &[][..] };

        Ok(Self {
            header: TsHeader {
                transport_error,
                payload_unit_start,
                transport_priority,
                pid,
                scrambling_control,
                has_adaptation,
                has_payload,
                continuity_counter,
            },
            adaptation,
            payload,
        })
    }
}
