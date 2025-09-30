use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<OutputEndCondition> for core::PipelineOutputEndCondition {
    type Error = TypeError;

    fn try_from(value: OutputEndCondition) -> Result<Self, Self::Error> {
        match value {
            OutputEndCondition {
                any_of: Some(any_of),
                all_of: None,
                any_input: None,
                all_inputs: None,
            } => Ok(core::PipelineOutputEndCondition::AnyOf(
                any_of.into_iter().map(Into::into).collect(),
            )),
            OutputEndCondition {
                any_of: None,
                all_of: Some(all_of),
                any_input: None,
                all_inputs: None,
            } => Ok(core::PipelineOutputEndCondition::AllOf(
                all_of.into_iter().map(Into::into).collect(),
            )),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: Some(true),
                all_inputs: None,
            } => Ok(core::PipelineOutputEndCondition::AnyInput),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: None,
                all_inputs: Some(true),
            } => Ok(core::PipelineOutputEndCondition::AllInputs),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: None | Some(false),
                all_inputs: None | Some(false),
            } => Ok(core::PipelineOutputEndCondition::Never),
            _ => Err(TypeError::new(
                "Only one of \"any_of, all_of, any_input or all_inputs\" is allowed.",
            )),
        }
    }
}

impl From<H264EncoderPreset> for core::FfmpegH264EncoderPreset {
    fn from(value: H264EncoderPreset) -> Self {
        match value {
            H264EncoderPreset::Ultrafast => core::FfmpegH264EncoderPreset::Ultrafast,
            H264EncoderPreset::Superfast => core::FfmpegH264EncoderPreset::Superfast,
            H264EncoderPreset::Veryfast => core::FfmpegH264EncoderPreset::Veryfast,
            H264EncoderPreset::Faster => core::FfmpegH264EncoderPreset::Faster,
            H264EncoderPreset::Fast => core::FfmpegH264EncoderPreset::Fast,
            H264EncoderPreset::Medium => core::FfmpegH264EncoderPreset::Medium,
            H264EncoderPreset::Slow => core::FfmpegH264EncoderPreset::Slow,
            H264EncoderPreset::Slower => core::FfmpegH264EncoderPreset::Slower,
            H264EncoderPreset::Veryslow => core::FfmpegH264EncoderPreset::Veryslow,
            H264EncoderPreset::Placebo => core::FfmpegH264EncoderPreset::Placebo,
        }
    }
}

impl From<OpusEncoderPreset> for core::OpusEncoderPreset {
    fn from(value: OpusEncoderPreset) -> Self {
        match value {
            OpusEncoderPreset::Quality => core::OpusEncoderPreset::Quality,
            OpusEncoderPreset::Voip => core::OpusEncoderPreset::Voip,
            OpusEncoderPreset::LowestLatency => core::OpusEncoderPreset::LowestLatency,
        }
    }
}

impl From<PixelFormat> for core::OutputPixelFormat {
    fn from(value: PixelFormat) -> Self {
        match value {
            PixelFormat::Yuv420p => core::OutputPixelFormat::YUV420P,
            PixelFormat::Yuv422p => core::OutputPixelFormat::YUV422P,
            PixelFormat::Yuv444p => core::OutputPixelFormat::YUV444P,
        }
    }
}

impl TryFrom<VulkanH264EncoderBitrate> for core::VulkanH264EncoderBitrate {
    type Error = TypeError;

    fn try_from(value: VulkanH264EncoderBitrate) -> Result<Self, Self::Error> {
        match value {
            VulkanH264EncoderBitrate::AverageBitrate(average_bitrate) => {
                Ok(core::VulkanH264EncoderBitrate {
                    average_bitrate,
                    max_bitrate: (average_bitrate as f64 * 1.25) as u64,
                })
            }
            VulkanH264EncoderBitrate::Vbr {
                average_bitrate,
                max_bitrate,
            } => {
                if average_bitrate > max_bitrate {
                    return Err(TypeError::new(
                        "max_bitrate has to be greater than average_bitrate",
                    ));
                }

                Ok(core::VulkanH264EncoderBitrate {
                    average_bitrate,
                    max_bitrate,
                })
            }
        }
    }
}
