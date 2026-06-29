use ash::vk;

use crate::parameters::{EncoderContent, EncoderUsage};

impl From<EncoderUsage> for vk::VideoEncodeUsageFlagsKHR {
    fn from(usage: EncoderUsage) -> Self {
        match usage {
            EncoderUsage::Default => Self::DEFAULT,
            EncoderUsage::Transcoding => Self::TRANSCODING,
            EncoderUsage::Streaming => Self::STREAMING,
            EncoderUsage::Recording => Self::RECORDING,
            EncoderUsage::Conferencing => Self::CONFERENCING,
        }
    }
}

impl From<EncoderContent> for vk::VideoEncodeContentFlagsKHR {
    fn from(content: EncoderContent) -> Self {
        match content {
            EncoderContent::Default => Self::DEFAULT,
            EncoderContent::Camera => Self::CAMERA,
            EncoderContent::Desktop => Self::DESKTOP,
            EncoderContent::Rendered => Self::RENDERED,
        }
    }
}
