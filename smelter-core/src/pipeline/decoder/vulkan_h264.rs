use std::sync::Arc;

use crossbeam_channel::Receiver;
use gpu_video::{
    H264DecoderEvent, OutputFrame, ReferenceManagementError, VideoDecoderError, VideoDeviceExt,
    WgpuTexturesDecoderH264,
    parameters::{CorruptedStateHandling, DecoderParameters, DecoderUsage},
};
use smelter_render::{FrameData, Resolution};
use tracing::{debug, info, trace, warn};

use crate::pipeline::decoder::{
    EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
};
use crate::prelude::*;

pub struct VulkanH264Decoder {
    decoder: WgpuTexturesDecoderH264,
    frame_receiver: Receiver<OutputFrame<wgpu::Texture>>,
    keyframe_request_sender: Option<KeyframeRequestSender>,
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
        let (frame_sender, frame_receiver) = crossbeam_channel::unbounded();
        let decoder = device.create_wgpu_textures_decoder_h264(
            &ctx.wgpu_ctx.queue,
            DecoderParameters {
                corrupted_state_handling: CorruptedStateHandling::Strict,
                usage_flags: DecoderUsage::Default,
                ..Default::default()
            },
            move |frame| {
                let _ = frame_sender.send(frame);
            },
        )?;
        Ok(Self {
            decoder,
            frame_receiver,
            keyframe_request_sender,
        })
    }
}

impl VideoDecoderInstance for VulkanH264Decoder {
    fn decode(&mut self, event: EncodedInputEvent) -> Vec<Frame> {
        trace!(?event, "Vulkan H264 decoder received an event.");

        let decoder_event = match &event {
            EncodedInputEvent::Chunk(chunk) => {
                H264DecoderEvent::DecodeChunk(gpu_video::EncodedInputChunk {
                    data: chunk.data.as_ref(),
                    pts: Some(chunk.pts.as_micros() as u64),
                })
            }
            EncodedInputEvent::LostData => H264DecoderEvent::SignalDataLoss,
            EncodedInputEvent::AuDelimiter => H264DecoderEvent::SignalFrameEnd,
            EncodedInputEvent::Discontinuity => H264DecoderEvent::Flush,
        };

        if let Err(err) = self.decoder.process_event(decoder_event, None) {
            match err {
                VideoDecoderError::ReferenceManagementError(
                    ReferenceManagementError::CorruptedState,
                ) => {
                    if let Some(s) = self.keyframe_request_sender.as_ref() {
                        s.send()
                    }
                    debug!("Vulkan H264 decoder detected a missing frame.");
                }
                err => warn!("Failed to decode frame: {err}"),
            }
        }

        self.drain_decoded_frames()
    }

    fn flush(&mut self) -> Vec<Frame> {
        if let Err(err) = self.decoder.flush() {
            warn!("Failed to flush the decoder: {err}");
        }

        self.drain_decoded_frames()
    }
}

impl VulkanH264Decoder {
    fn drain_decoded_frames(&self) -> Vec<Frame> {
        self.frame_receiver.try_iter().map(from_vk_frame).collect()
    }
}

fn from_vk_frame(frame: gpu_video::OutputFrame<wgpu::Texture>) -> Frame {
    let gpu_video::OutputFrame { data, metadata } = frame;
    let resolution = Resolution {
        width: data.width() as usize,
        height: data.height() as usize,
    };
    let pts = Timestamp::from_micros(metadata.pts.unwrap() as i64);

    trace!(?pts, "H264 Vulkan decoder produced a frame.");
    Frame {
        data: FrameData::Nv12WgpuTexture(data.into()),
        pts,
        resolution,
    }
}
