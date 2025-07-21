use std::{sync::Arc, time::Duration};

use compositor_render::{Frame, FrameData, Resolution};
use tracing::{info, warn};
use vk_video::{EncodedChunk, WgpuTexturesDecoder};

use crate::pipeline::decoder::{VideoDecoder, VideoDecoderInstance};
use crate::prelude::*;

pub struct VulkanH264Decoder {
    decoder: WgpuTexturesDecoder<'static>,
}

impl VideoDecoder for VulkanH264Decoder {
    const LABEL: &'static str = "Vulkan H264 decoder";

    fn new(ctx: &Arc<PipelineCtx>) -> Result<Self, DecoderInitError> {
        match &ctx.graphics_context.vulkan_ctx {
            Some(vulkan_ctx) => {
                info!("Initializing Vulkan H264 decoder");
                let device = vulkan_ctx.device.clone();
                let decoder = device.create_wgpu_textures_decoder()?;
                Ok(Self { decoder })
            }
            None => Err(DecoderInitError::VulkanContextRequiredForVulkanDecoder),
        }
    }
}

impl VideoDecoderInstance for VulkanH264Decoder {
    fn decode(&mut self, chunk: EncodedInputChunk) -> Vec<Frame> {
        let chunk = EncodedChunk {
            data: chunk.data.as_ref(),
            pts: Some(chunk.pts.as_micros() as u64),
        };

        let frames = match self.decoder.decode(chunk) {
            Ok(res) => res,
            Err(err) => {
                warn!("Failed to decode frame: {err}");
                return Vec::new();
            }
        };

        frames.into_iter().map(from_vk_frame).collect()
    }

    fn flush(&mut self) -> Vec<Frame> {
        self.decoder
            .flush()
            .into_iter()
            .map(from_vk_frame)
            .collect()
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
