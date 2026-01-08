use std::mem;

use bytes::Bytes;

use crate::{AudioTag, VideoTag, error::ParseError, tag::PacketType};

/// Parser for RTMP payload.
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

    /// Extracts codec data from payload of RTMP video message. Returns type of the parsed packet.
    /// The `data` must be payload of a RTMP video message.
    pub fn parse_video(&mut self, data: Bytes) -> Result<PacketType, ParseError> {
        let video_tag = VideoTag::parse(data)?;
        match video_tag.packet_type {
            PacketType::Data => {
                self.video.push(video_tag);
                Ok(PacketType::Data)
            }
            PacketType::Config => {
                self.avc_video_config = Some(video_tag);
                Ok(PacketType::Config)
            }
        }
    }

    /// Extracts codec data from payload of RTMP audio message. Returns type of the parsed packet.
    /// The `data` must be payload of a RTMP audio message.
    pub fn parse_audio(&mut self, data: Bytes) -> Result<PacketType, ParseError> {
        let audio_tag = AudioTag::parse(data)?;
        match audio_tag.packet_type {
            PacketType::Data => {
                self.audio.push(audio_tag);
                Ok(PacketType::Data)
            }
            PacketType::Config => {
                self.aac_audio_config = Some(audio_tag);
                Ok(PacketType::Config)
            }
        }
    }

    /// Returns parsed video tags. Moves parsed tags from struct.
    pub fn video(&mut self) -> Vec<VideoTag> {
        mem::take(&mut self.video)
    }

    /// Returns parsed audio tags. Moves parsed tags from struct.
    pub fn audio(&mut self) -> Vec<AudioTag> {
        mem::take(&mut self.audio)
    }

    /// Returns most recent video config received. Will return `Some` only if video codec is
    /// AVC.
    pub fn video_config(&self) -> &Option<VideoTag> {
        &self.avc_video_config
    }

    /// Returns most recent audio config received. Will return `Some` only if audio codec is
    /// AAC
    pub fn audio_config(&self) -> &Option<AudioTag> {
        &self.aac_audio_config
    }
}
