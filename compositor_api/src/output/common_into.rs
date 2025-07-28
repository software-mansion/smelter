use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<OutputEndCondition> for pipeline::PipelineOutputEndCondition {
    type Error = TypeError;

    fn try_from(value: OutputEndCondition) -> Result<Self, Self::Error> {
        match value {
            OutputEndCondition {
                any_of: Some(any_of),
                all_of: None,
                any_input: None,
                all_inputs: None,
            } => Ok(pipeline::PipelineOutputEndCondition::AnyOf(
                any_of.into_iter().map(Into::into).collect(),
            )),
            OutputEndCondition {
                any_of: None,
                all_of: Some(all_of),
                any_input: None,
                all_inputs: None,
            } => Ok(pipeline::PipelineOutputEndCondition::AllOf(
                all_of.into_iter().map(Into::into).collect(),
            )),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: Some(true),
                all_inputs: None,
            } => Ok(pipeline::PipelineOutputEndCondition::AnyInput),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: None,
                all_inputs: Some(true),
            } => Ok(pipeline::PipelineOutputEndCondition::AllInputs),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: None | Some(false),
                all_inputs: None | Some(false),
            } => Ok(pipeline::PipelineOutputEndCondition::Never),
            _ => Err(TypeError::new(
                "Only one of \"any_of, all_of, any_input or all_inputs\" is allowed.",
            )),
        }
    }
}

impl From<H264EncoderPreset> for pipeline::FfmpegH264EncoderPreset {
    fn from(value: H264EncoderPreset) -> Self {
        match value {
            H264EncoderPreset::Ultrafast => pipeline::FfmpegH264EncoderPreset::Ultrafast,
            H264EncoderPreset::Superfast => pipeline::FfmpegH264EncoderPreset::Superfast,
            H264EncoderPreset::Veryfast => pipeline::FfmpegH264EncoderPreset::Veryfast,
            H264EncoderPreset::Faster => pipeline::FfmpegH264EncoderPreset::Faster,
            H264EncoderPreset::Fast => pipeline::FfmpegH264EncoderPreset::Fast,
            H264EncoderPreset::Medium => pipeline::FfmpegH264EncoderPreset::Medium,
            H264EncoderPreset::Slow => pipeline::FfmpegH264EncoderPreset::Slow,
            H264EncoderPreset::Slower => pipeline::FfmpegH264EncoderPreset::Slower,
            H264EncoderPreset::Veryslow => pipeline::FfmpegH264EncoderPreset::Veryslow,
            H264EncoderPreset::Placebo => pipeline::FfmpegH264EncoderPreset::Placebo,
        }
    }
}

impl From<OpusEncoderPreset> for pipeline::OpusEncoderPreset {
    fn from(value: OpusEncoderPreset) -> Self {
        match value {
            OpusEncoderPreset::Quality => pipeline::OpusEncoderPreset::Quality,
            OpusEncoderPreset::Voip => pipeline::OpusEncoderPreset::Voip,
            OpusEncoderPreset::LowestLatency => pipeline::OpusEncoderPreset::LowestLatency,
        }
    }
}

impl From<PixelFormat> for pipeline::OutputPixelFormat {
    fn from(value: PixelFormat) -> Self {
        match value {
            PixelFormat::Yuv420p => pipeline::OutputPixelFormat::YUV420P,
            PixelFormat::Yuv422p => pipeline::OutputPixelFormat::YUV422P,
            PixelFormat::Yuv444p => pipeline::OutputPixelFormat::YUV444P,
        }
    }
}
