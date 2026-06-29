use ash::vk;

use crate::parameters::DecoderUsage;

impl From<DecoderUsage> for vk::VideoDecodeUsageFlagsKHR {
    fn from(usage: DecoderUsage) -> Self {
        match usage {
            DecoderUsage::Default => Self::DEFAULT,
            DecoderUsage::Transcoding => Self::TRANSCODING,
            DecoderUsage::Offline => Self::OFFLINE,
            DecoderUsage::Streaming => Self::STREAMING,
        }
    }
}
