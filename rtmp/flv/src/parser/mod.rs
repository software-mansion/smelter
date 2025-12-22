use std::mem;

use bytes::Bytes;

use crate::{AudioTag, ParseError, VideoTag, tag::PacketType};

pub mod audio;
pub mod error;
pub mod video;

#[derive(Debug, Default)]
pub struct RtmpParser {
    avc_video_config: Option<VideoTag>,
    aac_audio_config: Option<AudioTag>,

    video: Vec<VideoTag>,
    audio: Vec<AudioTag>,
}

impl RtmpParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse_video(&mut self, data: Bytes) -> Result<bool, ParseError> {
        let video_tag = VideoTag::parse(data)?;
        match video_tag.packet_type {
            PacketType::Data => {
                self.video.push(video_tag);
                Ok(false)
            }
            PacketType::Config => {
                self.avc_video_config = Some(video_tag);
                Ok(true)
            }
        }
    }

    pub fn parse_audio(&mut self, data: Bytes) -> Result<bool, ParseError> {
        let audio_tag = AudioTag::parse(data)?;
        match audio_tag.packet_type {
            PacketType::Data => {
                self.audio.push(audio_tag);
                Ok(false)
            }
            PacketType::Config => {
                self.aac_audio_config = Some(audio_tag);
                Ok(true)
            }
        }
    }

    pub fn video(&mut self) -> Vec<VideoTag> {
        mem::take(&mut self.video)
    }

    pub fn audio(&mut self) -> Vec<AudioTag> {
        mem::take(&mut self.audio)
    }
}
