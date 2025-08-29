use std::{num::NonZero, ops::Deref, sync::Arc};

use compositor_render::{FrameData, OutputFrameFormat};
use tracing::{error, info};
use vk_video::{RateControl, Rational, VideoParameters, WgpuTexturesEncoder};

use crate::{graphics_context::GraphicsContext, prelude::*};

use super::{VideoEncoder, VideoEncoderConfig};

pub struct VulkanH264Encoder {
    encoder: WgpuTexturesEncoder,
    ctx: GraphicsContext,
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
        let width = NonZero::new(u32::max(options.resolution.width as u32, 1)).unwrap();
        let height = NonZero::new(u32::max(options.resolution.height as u32, 1)).unwrap();
        let framerate = ctx.output_framerate;
        let bitrate = options.bitrate.unwrap_or_else(|| {
            let precision = 500_000.0; // 500kb
            let bpp = 0.08;

            let average_bitrate = (width.get() * height.get()) as f64
                * (framerate.num as f64 / framerate.den as f64)
                * bpp;
            let average_bitrate = (average_bitrate / precision).ceil() * precision;
            let max_bitrate = average_bitrate * 1.25;

            VulkanH264EncoderBitrate {
                average_bitrate: average_bitrate as u64,
                max_bitrate: max_bitrate as u64,
            }
        });
        let rate_control = RateControl::Vbr {
            average_bitrate: bitrate.average_bitrate,
            max_bitrate: bitrate.max_bitrate,
        };
        let device = vulkan_ctx.device.clone();

        let video_params = VideoParameters {
            width,
            height,
            target_framerate: Rational {
                numerator: framerate.num,
                denominator: NonZero::new(u32::max(framerate.den, 1)).unwrap(),
            },
        };

        let encoder_params = device.encoder_parameters_high_quality(video_params, rate_control);
        let encoder = device.create_wgpu_textures_encoder(encoder_params)?;

        Ok((
            Self {
                encoder,
                ctx: ctx.graphics_context.clone(),
            },
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

        transition_texture(&self.ctx, &texture);
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

fn transition_texture(ctx: &GraphicsContext, texture: &wgpu::Texture) {
    let mut command_encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    command_encoder.transition_resources(
        [].into_iter(),
        [wgpu::TextureTransition {
            texture,
            state: wgpu::TextureUses::RESOURCE,
            selector: None,
        }]
        .into_iter(),
    );

    ctx.queue.submit([command_encoder.finish()]);
}
