use std::mem;

use bytes::Bytes;

pub mod audio;
pub mod error;
pub mod header;
pub mod scriptdata;
pub mod tag;
pub mod video;

use crate::{
    Header, PacketType, ParseError,
    tag::{FlvTag, TagType},
};

#[derive(Debug, Default, Clone)]
pub struct Parser {
    header: Option<Header>,

    avc_decoder_config: Option<FlvTag>,
    aac_decoder_config: Option<FlvTag>,

    audio: Vec<FlvTag>,
    video: Vec<FlvTag>,
    script_data: Vec<FlvTag>,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse(&mut self, bytes: Bytes) -> Result<(), ParseError> {
        let next_data = match &self.header {
            Some(_) => {
                let (tag, next_data) = FlvTag::parse(bytes)?;
                match &tag.tag_type {
                    TagType::Audio(atag) => match atag.packet_type {
                        PacketType::Audio => self.audio.push(tag),
                        PacketType::AudioConfig => match self.aac_decoder_config {
                            Some(_) => return Err(ParseError::AacConfigDuplication),
                            None => self.aac_decoder_config = Some(tag),
                        },
                        _ => unreachable!(),
                    },
                    TagType::Video(vtag) => match vtag.packet_type {
                        PacketType::Video => self.video.push(tag),
                        PacketType::VideoConfig => match self.avc_decoder_config {
                            Some(_) => return Err(ParseError::AvcConfigDuplication),
                            None => self.avc_decoder_config = Some(tag),
                        },
                        _ => unreachable!(),
                    },
                    TagType::ScriptData(_) => self.script_data.push(tag),
                }
                next_data
            }
            None => {
                let (header, next_data) = Header::parse(bytes)?;
                self.header = Some(header);
                next_data
            }
        };

        if let Some(data) = next_data
            && data.len() > 4
        {
            self.parse(data)?;
        }

        Ok(())
    }

    /// Returns flv header or `None` if not parsed yet.
    pub fn header(&self) -> Option<Header> {
        self.header
    }

    pub fn take_audio(&mut self) -> Vec<FlvTag> {
        mem::take(&mut self.audio)
    }

    pub fn take_video(&mut self) -> Vec<FlvTag> {
        mem::take(&mut self.video)
    }

    pub fn take_script_data(&mut self) -> Vec<FlvTag> {
        mem::take(&mut self.script_data)
    }

    pub fn avc_decoder_config(&self) -> &Option<FlvTag> {
        &self.avc_decoder_config
    }

    pub fn aac_decoder_config(&self) -> &Option<FlvTag> {
        &self.aac_decoder_config
    }
}
