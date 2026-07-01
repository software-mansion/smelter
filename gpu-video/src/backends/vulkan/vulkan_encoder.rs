use ash::vk;

use crate::parameters::EncoderUsage;

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
