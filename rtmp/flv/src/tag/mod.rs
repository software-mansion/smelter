use bytes::Bytes;

use crate::{AudioTag, VideoTag};

pub mod audio;
pub mod video;

#[derive(Debug, Clone, Copy)]
pub struct Header {
    pub has_audio: bool,
    pub has_video: bool,
    pub data_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketType {
    Audio,
    AudioConfig,
    Video,
    VideoConfig,
    ScriptData,
}

#[derive(Debug, Clone)]
pub enum TagType {
    Audio(AudioTag),
    Video(VideoTag),
    ScriptData(ScriptDataTag),
}

#[derive(Debug, Clone)]
pub struct ScriptDataTag {
    pub payload: Bytes,
}

#[derive(Debug, Clone)]
pub struct FlvTag {
    pub tag_type: TagType,
    pub data_size: u32,
    pub timestamp: i32,
    pub previous_tag_size: u32,
}

impl FlvTag {
    pub fn payload(&self) -> Option<&Bytes> {
        match &self.tag_type {
            TagType::Audio(audio_tag) => Some(&audio_tag.payload),
            TagType::Video(video_tag) => Some(&video_tag.payload),
            TagType::ScriptData(_) => None,
        }
    }

    pub fn packet_type(&self) -> PacketType {
        match &self.tag_type {
            TagType::Video(v_tag) => v_tag.packet_type,
            TagType::Audio(a_tag) => a_tag.packet_type,
            TagType::ScriptData(_) => PacketType::ScriptData,
        }
    }
}
