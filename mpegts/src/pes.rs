//! PES (Packetized Elementary Stream) header parsing.

use crate::error::Error;

#[derive(Debug, Clone, Copy)]
pub struct PesHeader {
    pub stream_id: u8,
    /// Declared `PES_packet_length`. `0` means unbounded (common for video).
    pub declared_length: u16,
    pub pts: Option<u64>,
    pub dts: Option<u64>,
    /// Offset within the PES packet where the elementary-stream payload starts.
    pub payload_offset: usize,
}

impl PesHeader {
    pub fn parse(buf: &[u8]) -> Result<Self, Error> {
        if buf.len() < 6 {
            return Err(Error::PesTooShort);
        }
        if buf[0] != 0x00 || buf[1] != 0x00 || buf[2] != 0x01 {
            return Err(Error::InvalidPesStartCode);
        }
        let stream_id = buf[3];
        let declared_length = u16::from_be_bytes([buf[4], buf[5]]);

        // Stream IDs that skip the optional PES header extension
        // (ISO/IEC 13818-1 2.4.3.7).
        let skip_extension = matches!(
            stream_id,
            0xBC // program_stream_map
                | 0xBE // padding_stream
                | 0xBF // private_stream_2
                | 0xF0 // ECM
                | 0xF1 // EMM
                | 0xF2 // DSMCC
                | 0xF8 // ITU-T Rec. H.222.1 type E
                | 0xFF // program_stream_directory
        );
        if skip_extension {
            return Ok(Self {
                stream_id,
                declared_length,
                pts: None,
                dts: None,
                payload_offset: 6,
            });
        }

        if buf.len() < 9 {
            return Err(Error::PesTooShort);
        }
        let pts_dts_flags = (buf[7] >> 6) & 0x03;
        let header_data_length = buf[8] as usize;
        let payload_offset = 9 + header_data_length;
        if buf.len() < payload_offset {
            return Err(Error::PesTooShort);
        }

        let (pts, dts) = match pts_dts_flags {
            0b10 => (Some(read_timestamp(&buf[9..14])?), None),
            0b11 => {
                let pts = read_timestamp(&buf[9..14])?;
                let dts = read_timestamp(&buf[14..19])?;
                (Some(pts), Some(dts))
            }
            _ => (None, None),
        };

        Ok(Self {
            stream_id,
            declared_length,
            pts,
            dts,
            payload_offset,
        })
    }
}

fn read_timestamp(buf: &[u8]) -> Result<u64, Error> {
    if buf.len() < 5 {
        return Err(Error::PesTooShort);
    }
    // 33-bit timestamp split across 5 bytes with three marker bits.
    let ts = ((u64::from(buf[0]) >> 1) & 0x07) << 30
        | (u64::from(buf[1])) << 22
        | ((u64::from(buf[2]) >> 1) & 0x7F) << 15
        | (u64::from(buf[3])) << 7
        | ((u64::from(buf[4]) >> 1) & 0x7F);
    Ok(ts)
}
