//! Program Map Table.

use crate::{error::Error, stream_type::StreamType};

#[derive(Debug, Clone)]
pub struct Pmt {
    pub pcr_pid: u16,
    pub streams: Vec<PmtStream>,
}

#[derive(Debug, Clone, Copy)]
pub struct PmtStream {
    pub pid: u16,
    pub stream_type: StreamType,
}

impl Pmt {
    pub fn parse(section: &[u8]) -> Result<Self, Error> {
        if section.len() < 12 {
            return Err(Error::InvalidPsi);
        }
        if section[0] != 0x02 {
            return Err(Error::UnexpectedTableId(section[0]));
        }
        let section_length = ((u16::from(section[1] & 0x0F) << 8) | u16::from(section[2])) as usize;
        let total_len = 3 + section_length;
        if section.len() < total_len || section_length < 13 {
            return Err(Error::InvalidPsi);
        }

        let pcr_pid = u16::from_be_bytes([section[8], section[9]]) & 0x1FFF;
        let program_info_length =
            ((u16::from(section[10] & 0x0F) << 8) | u16::from(section[11])) as usize;

        let body_end = total_len - 4; // exclude CRC32
        let mut offset = 12 + program_info_length;
        if offset > body_end {
            return Err(Error::InvalidPsi);
        }

        let mut streams = Vec::new();
        while offset + 5 <= body_end {
            let stream_type = StreamType::from_u8(section[offset]);
            let pid = u16::from_be_bytes([section[offset + 1], section[offset + 2]]) & 0x1FFF;
            let es_info_length = ((u16::from(section[offset + 3] & 0x0F) << 8)
                | u16::from(section[offset + 4])) as usize;
            streams.push(PmtStream { pid, stream_type });
            offset += 5 + es_info_length;
            if offset > body_end {
                return Err(Error::InvalidPsi);
            }
        }

        Ok(Self { pcr_pid, streams })
    }
}
