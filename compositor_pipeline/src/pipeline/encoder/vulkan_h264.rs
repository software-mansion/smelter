use std::{num::NonZero, ops::Deref, sync::Arc};

use compositor_render::{FrameData, OutputFrameFormat};
use tracing::{error, info};
use vk_video::{RateControl, Rational, VideoParameters, WgpuTexturesEncoder};

use crate::prelude::*;

use super::{VideoEncoder, VideoEncoderConfig};

pub struct VulkanH264Encoder {
    encoder: WgpuTexturesEncoder,
}

impl VideoEncoder for VulkanH264Encoder {
    const LABEL: &'static str = "Vulkan H264 encoder";

    type Options = VulkanH264EncoderOptions;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        let Some(vulkan_ctx) = &ctx.graphics_context.vulkan_ctx else {
            return Err(EncoderInitError::VulkanContextRequiredForVulkanEncoder);
        };

        info!("Initializing Vulkan H264 encoder");
        let framerate = ctx.output_framerate;
        let rate_control = match options.rate_control {
            VulkanH264EncoderRateControl::EncoderDefault => RateControl::EncoderDefault,
            VulkanH264EncoderRateControl::Vbr {
                average_bitrate,
                max_bitrate,
            } => RateControl::Vbr {
                average_bitrate,
                max_bitrate,
            },
            VulkanH264EncoderRateControl::Disabled => RateControl::Disabled,
        };
        let device = vulkan_ctx.device.clone();

        let video_params = VideoParameters {
            width: NonZero::new(options.resolution.width as u32).unwrap(),
            height: NonZero::new(options.resolution.height as u32).unwrap(),
            target_framerate: Rational {
                numerator: framerate.num,
                denominator: NonZero::new(framerate.den).unwrap(),
            },
        };
        let encoder_params = match options.quality_level {
            VulkanH264EncoderQualityLevel::Low => {
                device.encoder_parameters_low_latency(video_params, rate_control)
            }
            VulkanH264EncoderQualityLevel::High => {
                device.encoder_parameters_high_quality(video_params, rate_control)
            }
        };
        let encoder = device.create_wgpu_textures_encoder(encoder_params)?;

        Ok((
            Self { encoder },
            VideoEncoderConfig {
                resolution: options.resolution,
                output_format: OutputFrameFormat::RgbaWgpuTexture,
                extradata: None,
            },
        ))
    }

    fn encode(&mut self, frame: Frame, force_keyframe: bool) -> Vec<EncodedOutputChunk> {
        let FrameData::Rgba8UnormWgpuTexture(texture) = frame.data else {
            error!("Unsupported pixel format {:?}. Dropping frame.", frame.data);
            return Vec::new();
        };

        let result = unsafe {
            self.encoder.encode(
                vk_video::Frame {
                    data: texture.deref().clone(),
                    pts: None,
                },
                force_keyframe,
            )
        };
        match result {
            Ok(chunk) => {
                vec![EncodedOutputChunk {
                    data: chunk.data.into(),
                    pts: frame.pts,
                    dts: None,
                    is_keyframe: chunk.is_keyframe,
                    kind: MediaKind::Video(VideoCodec::H264),
                }]
            }
            Err(err) => {
                error!("Encoder error: {err}.");
                Vec::new()
            }
        }
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        // Encoder does not store frames (this will change with B-frame support)
        Vec::new()
    }
}
