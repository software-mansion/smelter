use bytes::Bytes;

use crate::{
    AudioTag, ParseError, ScriptDataTag, VideoTag,
    tag::{FlvTag, TagType},
};

impl FlvTag {
    pub(super) fn parse(data: Bytes) -> Result<(Self, Option<Bytes>), ParseError> {
        if data.len() < 11 {
            return Err(ParseError::NotEnoughData);
        }

        let previous_tag_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let data = &data[4..];

        let filter = (data[0] >> 6) & 0x01;
        if filter != 0 {
            return Err(ParseError::UnsupportedFiltered);
        }

        let data_size = u32::from_be_bytes([0, data[1], data[2], data[3]]);

        // Explanation of weird byte order: https://veovera.org/docs/legacy/video-file-format-v10-1-spec.pdf#page=75
        let timestamp = i32::from_be_bytes([data[7], data[4], data[5], data[6]]);

        let tag_type = data[0] & 0x1F;

        let data = &data[11..];

        if data.len() < data_size as usize {
            return Err(ParseError::NotEnoughData);
        }
        let tag_data = &data[..(data_size as usize)];

        let tag_type = match tag_type {
            8 => TagType::Audio(AudioTag::parse(tag_data)?),
            9 => TagType::Video(VideoTag::parse(tag_data)?),
            18 => TagType::ScriptData(ScriptDataTag::parse(tag_data)),
            _ => return Err(ParseError::UnsupportedTagType(tag_type)),
        };

        let next_data = data.get((data_size as usize)..).map(Bytes::copy_from_slice);
        Ok((
            Self {
                tag_type,
                data_size,
                timestamp,
                previous_tag_size,
            },
            next_data,
        ))
    }
}
