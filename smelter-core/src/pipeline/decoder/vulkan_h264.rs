use std::{sync::Arc, time::Duration};

use crossbeam_channel::Receiver;
use gpu_video::{
    H264DecoderEvent, ReferenceManagementError, VideoDecoderError, VideoDeviceExt,
    WgpuTexturesDecoderH264,
    parameters::{DecoderParameters, DecoderUsage, MissedFrameHandling},
};
use smelter_render::{Frame, FrameData, Resolution};
use tracing::{debug, info, trace, warn};

use crate::pipeline::decoder::{
    EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
};
use crate::prelude::*;

type DecodeResult = Result<gpu_video::OutputFrame<wgpu::Texture>, VideoDecoderError>;

pub struct VulkanH264Decoder {
    decoder: WgpuTexturesDecoderH264,
    decode_result_receiver: Receiver<DecodeResult>,
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

        let (decode_result_sender, decode_result_receiver) = crossbeam_channel::unbounded();
        let on_frame = move |frame_result| {
            if let Err(err) = decode_result_sender.send(frame_result) {
                debug!("Failed to send decoded frame via channel: {err}")
            }
        };

        let decoder = device.create_wgpu_textures_decoder_h264(
            &ctx.wgpu_ctx.queue,
            DecoderParameters {
                missed_frame_handling: MissedFrameHandling::Strict,
                usage_flags: DecoderUsage::Default,
                ..Default::default()
            },
            on_frame,
        )?;

        Ok(Self {
            decoder,
            decode_result_receiver,
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

        let mut request_keyframe = false;
        let mut handle_decode_err = |err| match err {
            VideoDecoderError::ReferenceManagementError(ReferenceManagementError::MissingFrame) => {
                request_keyframe = true;
            }
            err => {
                warn!("Failed to decode frame: {err}");
            }
        };

        if let Err(err) = self.decoder.process_event(decoder_event) {
            handle_decode_err(err);
        }

        let mut frames = Vec::new();
        for result in self.decode_result_receiver.try_iter() {
            let frame = match result {
                Ok(frame) => frame,
                Err(err) => {
                    handle_decode_err(err);
                    continue;
                }
            };

            frames.push(from_vk_frame(frame));
        }

        if request_keyframe {
            if let Some(s) = self.keyframe_request_sender.as_ref() {
                s.send()
            }
            debug!("Vulkan H264 decoder detected a missing frame.");
        }

        match self.drop_frames {
            true => Vec::new(),
            false => frames,
        }
    }

    fn flush(&mut self) -> Vec<Frame> {
        if self.drop_frames {
            return Vec::new();
        }

        if let Err(err) = self.decoder.flush() {
            warn!("Failed to flush the decoder: {err}");
            return Vec::new();
        }

        self.decode_result_receiver
            .try_iter()
            .filter_map(|result| match result {
                Ok(frame) => Some(from_vk_frame(frame)),
                Err(err) => {
                    warn!("Failed to decode frame: {err}");
                    None
                }
            })
            .collect()
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
