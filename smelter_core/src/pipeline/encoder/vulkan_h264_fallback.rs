use std::sync::Arc;
use tracing::error;

use crate::prelude::*;

use super::{VideoEncoder, VideoEncoderConfig};

pub struct VulkanH264Encoder;

impl VideoEncoder for VulkanH264Encoder {
    const LABEL: &'static str = "Vulkan H264 encoder";

    type Options = VulkanH264EncoderOptions;

    fn new(
        _ctx: &Arc<PipelineCtx>,
        _options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        Err(EncoderInitError::VulkanContextRequiredForVulkanEncoder)
    }

    fn encode(&mut self, _frame: Frame, _force_keyframe: bool) -> Vec<EncodedOutputChunk> {
        error!("Vulkan encoder unavailable, this code should never be called");
        Vec::new()
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        error!("Vulkan encoder unavailable, this code should never be called");
        Vec::new()
    }
}
