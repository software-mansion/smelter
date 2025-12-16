use bytes::Bytes;

use crate::{Header, ParseError};

const DEFAULT_HEADER_SIZE: u32 = 9;

impl Header {
    pub(super) fn parse(data: Bytes) -> Result<(Self, Option<Bytes>), ParseError> {
        if data.len() < (DEFAULT_HEADER_SIZE) as usize {
            return Err(ParseError::NotEnoughData);
        }

        let [f, l, v] = data[0..3] else {
            return Err(ParseError::InvalidHeader);
        };

        let version = data[3];

        let has_audio = ((data[4] >> 2) & 0x01) == 1;
        let has_video = (data[4] & 0x01) == 1;

        let data_offset = u32::from_be_bytes([data[5], data[6], data[7], data[8]]);

        // These bytes match to ascii F, L, V
        if (f, l, v) != (0x46, 0x4C, 0x56) || version != 1 {
            return Err(ParseError::InvalidHeader);
        }

        let next_data = data
            .get((data_offset as usize)..)
            .map(Bytes::copy_from_slice);
        Ok((
            Header {
                has_audio,
                has_video,
                data_offset,
            },
            next_data,
        ))
    }
}
