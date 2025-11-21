use std::sync::Arc;

use smelter_render::Frame;
use tracing::error;

use crate::pipeline::decoder::{KeyframeRequestSender, VideoDecoder, VideoDecoderInstance};
use crate::prelude::*;

pub struct VulkanH264Decoder;

impl VideoDecoder for VulkanH264Decoder {
    const LABEL: &'static str = "Vulkan H264 decoder";

    fn new(
        _ctx: &Arc<PipelineCtx>,
        _keyframe_request_sender: Option<KeyframeRequestSender>,
    ) -> Result<Self, DecoderInitError> {
        Err(DecoderInitError::VulkanContextRequiredForVulkanDecoder)
    }
}

impl VideoDecoderInstance for VulkanH264Decoder {
    fn decode(&mut self, _chunk: EncodedInputChunk) -> Vec<Frame> {
        error!("Vulkan decoder unavailable, this code should never be called");
        vec![]
    }

    fn flush(&mut self) -> Vec<Frame> {
        error!("Vulkan decoder unavailable, this code should never be called");
        vec![]
    }

    fn skip_until_keyframe(&mut self) {}
}
