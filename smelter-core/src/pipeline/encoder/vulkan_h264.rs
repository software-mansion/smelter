use std::{
    num::NonZero,
    sync::{Arc, mpsc},
    time::Duration,
};

use gpu_video::{
    InputFrame, VideoDeviceExt, WgpuTexturesEncoderH264,
    parameters::{EncoderParametersH264, RateControl, Rational, VideoParameters},
};
use smelter_render::{FrameData, OutputFrameFormat, WgpuCtx};
use tracing::{error, info};

use crate::{
    pipeline::encoder::utils::{bitrate_from_resolution_framerate, gop_size_from_ms_framerate},
    pipeline::utils::{annexb_to_avcc, build_avc_decoder_config},
    prelude::*,
};

use super::{VideoEncoder, VideoEncoderConfig};

pub struct VulkanH264Encoder {
    encoder: WgpuTexturesEncoderH264,
    chunk_receiver: mpsc::Receiver<gpu_video::EncodedOutputChunk<Vec<u8>>>,
    wgpu_ctx: Arc<WgpuCtx>,
    bitstream_format: H264BitstreamFormat,
}

impl VideoEncoder for VulkanH264Encoder {
    const LABEL: &'static str = "Vulkan H264 encoder";

    type Options = VulkanH264EncoderOptions;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        if ctx.graphics_context.vulkan_ctx.is_none() {
            return Err(EncoderInitError::VulkanContextRequiredForVulkanEncoder);
        };

        info!("Initializing Vulkan H264 encoder");
        let width = NonZero::new(u32::max(options.resolution.width as u32, 1)).unwrap();
        let height = NonZero::new(u32::max(options.resolution.height as u32, 1)).unwrap();
        let framerate = ctx.output_framerate;
        let bitrate = options.bitrate.unwrap_or_else(|| {
            VulkanH264EncoderRateControl::VariableBitrate(bitrate_from_resolution_framerate(
                options.resolution,
                framerate,
            ))
        });

        let rate_control = match bitrate {
            VulkanH264EncoderRateControl::VariableBitrate(bitrate) => {
                RateControl::VariableBitrate {
                    average_bitrate: bitrate.average_bitrate,
                    max_bitrate: bitrate.max_bitrate,
                    virtual_buffer_size: std::time::Duration::from_secs(2),
                }
            }
            VulkanH264EncoderRateControl::ConstantBitrate(bitrate) => {
                RateControl::ConstantBitrate {
                    bitrate,
                    virtual_buffer_size: std::time::Duration::from_secs(2),
                }
            }
        };

        let device = ctx
            .wgpu_ctx
            .device
            .video()
            .map_err(|_| EncoderInitError::VulkanContextRequiredForVulkanEncoder)?;

        let video_params = VideoParameters {
            width,
            height,
            target_framerate: Rational {
                numerator: framerate.num,
                denominator: NonZero::new(u32::max(framerate.den, 1)).unwrap(),
            },
        };

        let mut encoder_params = match options.preset {
            VulkanH264EncoderPreset::HighQuality => EncoderParametersH264 {
                input_parameters: video_params,
                output_parameters: device
                    .encoder_output_parameters_h264_high_quality(rate_control)?,
            },
            VulkanH264EncoderPreset::LowLatency => EncoderParametersH264 {
                input_parameters: video_params,
                output_parameters: device
                    .encoder_output_parameters_h264_low_latency(rate_control)?,
            },
        };

        let gop_size_raw = gop_size_from_ms_framerate(options.keyframe_interval, framerate) as u32;
        let gop_size = NonZero::new(gop_size_raw).unwrap_or(NonZero::new(1).unwrap());

        encoder_params.output_parameters.idr_period = Some(gop_size);
        encoder_params.output_parameters.color_space =
            Some(gpu_video::parameters::ColorSpace::BT709);
        encoder_params.output_parameters.color_range =
            Some(gpu_video::parameters::ColorRange::Limited);

        if options.bitstream_format == H264BitstreamFormat::Avcc {
            encoder_params.output_parameters.inline_stream_params = Some(false);
        }

        let (chunk_sender, chunk_receiver) = mpsc::channel();
        let encoder = device.create_wgpu_textures_encoder_h264(
            &ctx.wgpu_ctx.queue,
            encoder_params,
            move |chunk| {
                if chunk_sender.send(chunk).is_err() {
                    error!("Vulkan H264 encoder dropped, discarding encoded chunk.");
                }
            },
        )?;

        let extradata = if options.bitstream_format == H264BitstreamFormat::Avcc {
            build_avc_decoder_config(&[encoder.sps()?, encoder.pps()?].concat())
        } else {
            None
        };

        Ok((
            Self {
                encoder,
                chunk_receiver,
                wgpu_ctx: ctx.wgpu_ctx.clone(),
                bitstream_format: options.bitstream_format,
            },
            VideoEncoderConfig {
                resolution: options.resolution,
                output_format: OutputFrameFormat::Nv12WgpuTexture,
                extradata,
            },
        ))
    }

    fn encode(&mut self, frame: Frame, force_keyframe: bool) -> Vec<EncodedOutputChunk> {
        let FrameData::Nv12WgpuTexture(texture) = frame.data else {
            error!("Unsupported pixel format {:?}. Dropping frame.", frame.data);
            return self.collect_chunks();
        };

        let input_texture = match self.encoder.input_texture() {
            Ok(texture) => texture,
            Err(err) => {
                error!("Failed to get encoder input texture: {err}. Dropping frame.");
                return self.collect_chunks();
            }
        };

        // TODO: avoid this copy
        let mut command_encoder =
            self.wgpu_ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Vulkan H264 encoder input copy"),
                });
        command_encoder.copy_texture_to_texture(
            texture.as_image_copy(),
            input_texture.as_image_copy(),
            texture.size(),
        );
        self.wgpu_ctx.queue.submit([command_encoder.finish()]);

        let result = self.encoder.encode(
            InputFrame {
                data: input_texture,
                pts: Some(frame.pts.as_micros() as u64),
            },
            force_keyframe,
        );
        if let Err(err) = result {
            error!("Encoder error: {err}.");
        }

        self.collect_chunks()
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        if let Err(err) = self.encoder.flush() {
            error!("Failed to flush encoder: {err}.");
        }
        self.collect_chunks()
    }
}

impl VulkanH264Encoder {
    fn collect_chunks(&mut self) -> Vec<EncodedOutputChunk> {
        self.chunk_receiver
            .try_iter()
            .map(|chunk| {
                let data = if self.bitstream_format == H264BitstreamFormat::Avcc {
                    annexb_to_avcc(&chunk.data)
                } else {
                    chunk.data.into()
                };
                EncodedOutputChunk {
                    data,
                    pts: Duration::from_micros(chunk.pts.unwrap_or(0)),
                    dts: None,
                    is_keyframe: chunk.is_keyframe,
                    kind: MediaKind::Video(VideoCodec::H264),
                }
            })
            .collect()
    }
}
