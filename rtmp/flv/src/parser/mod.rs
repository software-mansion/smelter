use std::mem;

use bytes::Bytes;

use crate::{AudioTag, ParseError, VideoTag};

pub mod audio;
pub mod error;
pub mod video;

#[derive(Debug, Default)]
pub struct RtmpParser {
    video: Vec<VideoTag>,
    audio: Vec<AudioTag>,
}

impl RtmpParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse_video(&mut self, data: Bytes) -> Result<(), ParseError> {
        let video_tag = VideoTag::parse(data)?;

        self.video.push(video_tag);
        Ok(())
    }

    pub fn parse_audio(&mut self, data: Bytes) -> Result<(), ParseError> {
        let audio_tag = AudioTag::parse(data)?;

        self.audio.push(audio_tag);
        Ok(())
    }

    pub fn video(&mut self) -> Vec<VideoTag> {
        mem::take(&mut self.video)
    }

    pub fn audio(&mut self) -> Vec<AudioTag> {
        mem::take(&mut self.audio)
    }
}
