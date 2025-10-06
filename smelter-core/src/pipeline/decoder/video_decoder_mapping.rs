use crate::codecs::VideoDecoderOptions;

#[derive(Debug, Clone)]
pub struct VideoDecoderMapping {
    pub h264: Option<VideoDecoderOptions>,
    pub vp8: Option<VideoDecoderOptions>,
    pub vp9: Option<VideoDecoderOptions>,
}

impl VideoDecoderMapping {
    pub fn has_any_codec(&self) -> bool {
        self.h264.is_some() || self.vp8.is_some() || self.vp9.is_some()
    }
}
