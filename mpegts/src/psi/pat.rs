//! Program Association Table.

use crate::error::Error;

#[derive(Debug, Clone)]
pub struct Pat {
    pub programs: Vec<ProgramEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct ProgramEntry {
    pub program_number: u16,
    /// `network_PID` when `program_number == 0`, otherwise `program_map_PID`.
    pub pid: u16,
}

impl Pat {
    pub fn parse(section: &[u8]) -> Result<Self, Error> {
        if section.len() < 3 {
            return Err(Error::InvalidPsi);
        }
        if section[0] != 0x00 {
            return Err(Error::UnexpectedTableId(section[0]));
        }
        let section_length = ((u16::from(section[1] & 0x0F) << 8) | u16::from(section[2])) as usize;
        let total_len = 3 + section_length;
        if section.len() < total_len || section_length < 9 {
            return Err(Error::InvalidPsi);
        }

        // Body lies between the 8-byte common header and the 4-byte CRC32.
        let body = &section[8..total_len - 4];
        if !body.len().is_multiple_of(4) {
            return Err(Error::InvalidPsi);
        }

        let programs = body
            .chunks_exact(4)
            .map(|c| ProgramEntry {
                program_number: u16::from_be_bytes([c[0], c[1]]),
                pid: u16::from_be_bytes([c[2], c[3]]) & 0x1FFF,
            })
            .collect();

        Ok(Self { programs })
    }
}
