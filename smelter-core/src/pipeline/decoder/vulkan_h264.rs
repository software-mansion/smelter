use std::{sync::Arc, time::Duration};

use gpu_video::{
    H264DecoderEvent, ReferenceManagementError, VideoDecoderError, VideoDeviceExt,
    WgpuTexturesDecoder,
    parameters::{DecoderParameters, DecoderUsageFlags, MissedFrameHandling},
};
use smelter_render::{Frame, FrameData, Resolution};
use tracing::{debug, info, trace, warn};

use crate::pipeline::decoder::{
    EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
};
use crate::prelude::*;

pub struct VulkanH264Decoder {
    decoder: WgpuTexturesDecoder,
    keyframe_request_sender: Option<KeyframeRequestSender>,
    drop_frames: bool,
}

impl VideoDecoder for VulkanH264Decoder {
    const LABEL: &'static str = "Vulkan H264 decoder";

    fn new(
        ctx: &Arc<PipelineCtx>,
        keyframe_request_sender: Option<KeyframeRequestSender>,
    ) -> Result<Self, DecoderInitError> {
        if ctx.graphics_context.vulkan_ctx.is_none() {
            return Err(DecoderInitError::VulkanContextRequiredForVulkanDecoder);
        }

        info!("Initializing Vulkan H264 decoder");
        let device = ctx
            .wgpu_ctx
            .device
            .video()
            .map_err(|_| DecoderInitError::VulkanContextRequiredForVulkanDecoder)?;
        let decoder = device.create_wgpu_textures_decoder_h264(DecoderParameters {
            missed_frame_handling: MissedFrameHandling::Strict,
            usage_flags: DecoderUsageFlags::DEFAULT,
        })?;
        Ok(Self {
            decoder,
            keyframe_request_sender,
            drop_frames: false,
        })
    }
}

impl VideoDecoderInstance for VulkanH264Decoder {
    fn decode(&mut self, event: EncodedInputEvent) -> Vec<Frame> {
        trace!(?event, "Vulkan H264 decoder received an event.");

        let decoder_event = match &event {
            EncodedInputEvent::Chunk(chunk) => {
                self.drop_frames = !chunk.present;
                H264DecoderEvent::DecodeChunk(gpu_video::EncodedInputChunk {
                    data: chunk.data.as_ref(),
                    pts: Some(chunk.pts.as_micros() as u64),
                })
            }
            EncodedInputEvent::LostData => H264DecoderEvent::SignalDataLoss,
            EncodedInputEvent::AuDelimiter => H264DecoderEvent::SignalFrameEnd,
        };

        let frames = match self.decoder.process_event(decoder_event) {
            Ok(frames) => frames,
            Err(VideoDecoderError::ReferenceManagementError(
                ReferenceManagementError::MissingFrame,
            )) => {
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

        match self.drop_frames {
            true => Vec::new(),
            false => frames.into_iter().map(from_vk_frame).collect(),
        }
    }

    fn flush(&mut self) -> Vec<Frame> {
        if self.drop_frames {
            return Vec::new();
        }
        match self.decoder.flush() {
            Ok(frames) => frames.into_iter().map(from_vk_frame).collect(),
            Err(err) => {
                warn!("Failed to flush the decoder: {err}");
                Vec::new()
            }
        }
    }
}

fn from_vk_frame(frame: gpu_video::OutputFrame<wgpu::Texture>) -> Frame {
    let gpu_video::OutputFrame { data, metadata } = frame;
    let resolution = Resolution {
        width: data.width() as usize,
        height: data.height() as usize,
    };
    let pts = Duration::from_micros(metadata.pts.unwrap());

    trace!(?pts, "H264 Vulkan decoder produced a frame.");
    Frame {
        data: FrameData::Nv12WgpuTexture(data.into()),
        pts,
        resolution,
    }
}
