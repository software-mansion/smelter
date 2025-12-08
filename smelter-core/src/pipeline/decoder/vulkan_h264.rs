use std::{sync::Arc, time::Duration};

use smelter_render::{Frame, FrameData, Resolution};
use tracing::{debug, info, trace, warn};
use vk_video::{
    DecoderError, ReferenceManagementError, WgpuTexturesDecoder,
    parameters::{DecoderParameters, DecoderUsageFlags, MissedFrameHandling},
};

use crate::pipeline::decoder::{
    EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
};
use crate::prelude::*;

pub struct VulkanH264Decoder {
    decoder: WgpuTexturesDecoder,
    keyframe_request_sender: Option<KeyframeRequestSender>,
}

impl VideoDecoder for VulkanH264Decoder {
    const LABEL: &'static str = "Vulkan H264 decoder";

    fn new(
        ctx: &Arc<PipelineCtx>,
        keyframe_request_sender: Option<KeyframeRequestSender>,
    ) -> Result<Self, DecoderInitError> {
        match &ctx.graphics_context.vulkan_ctx {
            Some(vulkan_ctx) => {
                info!("Initializing Vulkan H264 decoder");
                let device = vulkan_ctx.device.clone();
                let decoder = device.create_wgpu_textures_decoder(DecoderParameters {
                    missed_frame_handling: MissedFrameHandling::Strict,
                    usage_flags: DecoderUsageFlags::DEFAULT,
                })?;
                Ok(Self {
                    decoder,
                    keyframe_request_sender,
                })
            }
            None => Err(DecoderInitError::VulkanContextRequiredForVulkanDecoder),
        }
    }
}

impl VideoDecoderInstance for VulkanH264Decoder {
    fn decode(&mut self, event: EncodedInputEvent) -> Vec<Frame> {
        trace!(?event, "Vulkan H264 decoder received an event.");

        let chunk = match &event {
            EncodedInputEvent::Chunk(chunk) => vk_video::EncodedInputChunk {
                data: chunk.data.as_ref(),
                pts: Some(chunk.pts.as_micros() as u64),
            },
            EncodedInputEvent::LostData => {
                self.decoder.mark_missing_data();
                return vec![];
            }
            EncodedInputEvent::AuDelimiter => {
                return vec![];
            }
        };

        let frames = match self.decoder.decode(chunk) {
            Ok(res) => res,
            Err(DecoderError::ReferenceManagementError(ReferenceManagementError::MissingFrame)) => {
                if let Some(s) = self.keyframe_request_sender.as_ref() {
                    s.send()
                }
                debug!("Vulkan H264 decoder detected a missing frame.");
                return Vec::new();
            }
            Err(err) => {
                warn!("Failed to decode frame: {err}");
                return Vec::new();
            }
        };

        frames.into_iter().map(from_vk_frame).collect()
    }

    fn flush(&mut self) -> Vec<Frame> {
        match self.decoder.flush() {
            Ok(frames) => frames.into_iter().map(from_vk_frame).collect(),
            Err(err) => {
                warn!("Failed to flush the decoder: {err}");
                Vec::new()
            }
        }
    }
}

fn from_vk_frame(frame: vk_video::Frame<wgpu::Texture>) -> Frame {
    let vk_video::Frame { data, pts } = frame;
    let resolution = Resolution {
        width: data.width() as usize,
        height: data.height() as usize,
    };

    Frame {
        data: FrameData::Nv12WgpuTexture(data.into()),
        pts: Duration::from_micros(pts.unwrap()),
        resolution,
    }
}
