use crate::codecs::VideoDecoderOptions;

#[derive(Debug, Clone)]
pub struct NegotiatedVideoCodecsInfo {
    pub h264: Option<VideoCodecInfo>,
    pub vp8: Option<VideoCodecInfo>,
    pub vp9: Option<VideoCodecInfo>,
}

#[derive(Debug, Clone)]
pub struct NegotiatedAudioCodecsInfo {
    #[allow(dead_code)]
    pub opus: Option<AudioCodecInfo>,
}

#[derive(Debug, Clone)]
pub struct VideoCodecInfo {
    pub payload_types: Vec<u8>,
    pub preferred_decoder: VideoDecoderOptions,
}

#[derive(Debug, Clone)]
pub struct AudioCodecInfo {
    #[allow(dead_code)]
    pub payload_types: Vec<u8>,
}

impl NegotiatedVideoCodecsInfo {
    pub fn is_payload_type_h264(&self, pt: u8) -> bool {
        matches!(&self.h264, Some(info) if info.payload_types.contains(&pt))
    }

    pub fn is_payload_type_vp8(&self, pt: u8) -> bool {
        matches!(&self.vp8, Some(info) if info.payload_types.contains(&pt))
    }

    pub fn is_payload_type_vp9(&self, pt: u8) -> bool {
        matches!(&self.vp9, Some(info) if info.payload_types.contains(&pt))
    }

    pub fn has_any_codec(&self) -> bool {
        self.h264.is_some() || self.vp8.is_some() || self.vp9.is_some()
    }
}
