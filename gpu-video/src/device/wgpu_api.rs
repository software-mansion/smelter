use std::sync::Arc;

use crate::{
    DecoderError, VideoDeviceExt, VulkanEncoderError,
    device::{DecoderParameters, EncoderParametersH264, EncoderParametersH265},
    global_registry::GlobalRegistry,
    parser::{h264::H264Parser, reference_manager::ReferenceContext},
    vulkan_decoder::{FrameSorter, ImageModifiers, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
};

// TODO: add methods from VideoDeviceExt as well?
/// [`wgpu::Device`] extension that exposes video capabilities of a device.
/// The device must be created with [`VideoAdapterExt::request_device_with_video_support`](`crate::VideoAdapterExt::request_device_with_video_support`).
pub trait WgpuVideoDeviceExt: VideoDeviceExt {
    fn create_wgpu_textures_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<crate::WgpuTexturesDecoder, DecoderError>;

    fn create_wgpu_textures_encoder_h264(
        &self,
        queue: &wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, VulkanEncoderError>;

    fn create_wgpu_textures_encoder_h265(
        &self,
        queue: &wgpu::Queue,
        parameters: EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, VulkanEncoderError>;
}

impl WgpuVideoDeviceExt for wgpu::Device {
    fn create_wgpu_textures_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<crate::WgpuTexturesDecoder, DecoderError> {
        let video_device = GlobalRegistry::get_device(&self.into())?;

        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::new(parameters.missed_frame_handling);

        let vulkan_decoder = VulkanDecoder::new(
            Arc::new(video_device.decoding_device()?),
            parameters.usage_flags,
            ImageModifiers {
                additional_queue_index: video_device.queues.transfer.family_index,
                create_flags: Default::default(),
                usage_flags: Default::default(),
            },
        )?;
        let frame_sorter = FrameSorter::<wgpu::Texture>::new();

        Ok(crate::WgpuTexturesDecoder {
            wgpu_device: self.clone(),
            parser,
            reference_ctx,
            vulkan_decoder,
            frame_sorter,
        })
    }

    fn create_wgpu_textures_encoder_h264(
        &self,
        queue: &wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, VulkanEncoderError> {
        let video_device = GlobalRegistry::get_device(&self.into())?;
        let parameters = video_device.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(video_device.encoding_device()?), parameters)?;
        Ok(crate::WgpuTexturesEncoderH264 {
            wgpu_device: self.clone(),
            wgpu_queue: queue.clone(),
            vulkan_encoder: encoder,
        })
    }

    fn create_wgpu_textures_encoder_h265(
        &self,
        queue: &wgpu::Queue,
        parameters: EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, VulkanEncoderError> {
        let video_device = GlobalRegistry::get_device(&self.into())?;
        let parameters = video_device.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(video_device.encoding_device()?), parameters)?;
        Ok(crate::WgpuTexturesEncoderH265 {
            wgpu_device: self.clone(),
            wgpu_queue: queue.clone(),
            vulkan_encoder: encoder,
        })
    }
}
