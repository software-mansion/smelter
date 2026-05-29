use std::{
    ffi::CStr,
    sync::{Arc, OnceLock},
};

#[cfg(feature = "transcoder")]
use crate::parameters::TranscoderParameters;
use crate::{
    BytesDecoder, BytesEncoderH264, BytesEncoderH265, DecoderError, RegistryError, VideoDevice,
    VulkanEncoderError, WgpuInitError,
    capabilities::{DecodeCapabilities, EncodeCapabilities},
    device::{
        DecoderParameters, EncoderOutputParameters, EncoderParametersH264, EncoderParametersH265,
        VideoDeviceDescriptor,
    },
    parameters::{H264Profile, H265Profile, RateControl},
    parser::{h264::H264Parser, reference_manager::ReferenceContext},
    vulkan_decoder::{FrameSorter, ImageModifiers, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
    wgpu_helpers::{GlobalRegistry, VideoDeviceKey},
};

pub trait VideoDeviceExt {
    fn create_bytes_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, DecoderError>;
    #[cfg(feature = "wgpu")]
    fn create_wgpu_textures_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<crate::WgpuTexturesDecoder, DecoderError>;

    /// Create a single-input multiple-output transcoder.
    /// Each item in `parameters.output_parameters` corresponds to one output.
    #[cfg(feature = "transcoder")]
    fn create_transcoder(
        &self,
        parameters: TranscoderParameters,
    ) -> Result<crate::vulkan_transcoder::Transcoder, crate::vulkan_transcoder::TranscoderError>;

    fn create_bytes_encoder_h264(
        &self,
        parameters: EncoderParametersH264,
    ) -> Result<BytesEncoderH264, VulkanEncoderError>;
    fn create_bytes_encoder_h265(
        &self,
        parameters: EncoderParametersH265,
    ) -> Result<BytesEncoderH265, VulkanEncoderError>;
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

    fn decode_capabilities(&self) -> Result<DecodeCapabilities, crate::RegistryError>;
    fn encode_capabilities(&self) -> Result<EncodeCapabilities, crate::RegistryError>;

    fn encoder_output_parameters_h265_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VulkanEncoderError>;
    fn encoder_output_parameters_h264_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VulkanEncoderError>;
    fn encoder_output_parameters_h265_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VulkanEncoderError>;
    fn encoder_output_parameters_h264_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VulkanEncoderError>;

    fn supports_decoding(&self) -> Result<bool, crate::RegistryError>;
    fn supports_encoding(&self) -> Result<bool, crate::RegistryError>;
}

impl VideoDeviceExt for wgpu::Device {
    fn create_bytes_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, DecoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        ctx.create_bytes_decoder_h264(parameters)
    }

    fn create_wgpu_textures_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<crate::WgpuTexturesDecoder, DecoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;

        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::new(parameters.missed_frame_handling);

        let vulkan_decoder = VulkanDecoder::new(
            Arc::new(ctx.decoding_device()?),
            parameters.usage_flags,
            ImageModifiers {
                additional_queue_index: ctx.queues.transfer.family_index,
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

    #[cfg(feature = "transcoder")]
    fn create_transcoder(
        &self,
        parameters: TranscoderParameters,
    ) -> Result<crate::vulkan_transcoder::Transcoder, crate::vulkan_transcoder::TranscoderError>
    {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        ctx.create_transcoder(parameters)
    }

    fn create_bytes_encoder_h264(
        &self,
        parameters: EncoderParametersH264,
    ) -> Result<BytesEncoderH264, VulkanEncoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        ctx.create_bytes_encoder_h264(parameters)
    }

    fn create_bytes_encoder_h265(
        &self,
        parameters: EncoderParametersH265,
    ) -> Result<BytesEncoderH265, VulkanEncoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        ctx.create_bytes_encoder_h265(parameters)
    }

    fn create_wgpu_textures_encoder_h264(
        &self,
        queue: &wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, VulkanEncoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        let parameters = ctx.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(ctx.encoding_device()?), parameters)?;
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
        let ctx = GlobalRegistry::get_device(&self.into())?;
        let parameters = ctx.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(ctx.encoding_device()?), parameters)?;
        Ok(crate::WgpuTexturesEncoderH265 {
            wgpu_device: self.clone(),
            wgpu_queue: queue.clone(),
            vulkan_encoder: encoder,
        })
    }

    fn decode_capabilities(&self) -> Result<DecodeCapabilities, RegistryError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        Ok(ctx.decode_capabilities())
    }

    fn encode_capabilities(&self) -> Result<EncodeCapabilities, RegistryError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        Ok(ctx.encode_capabilities())
    }

    fn encoder_output_parameters_h265_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VulkanEncoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        ctx.encoder_output_parameters_h265_low_latency(rate_control)
    }

    fn encoder_output_parameters_h264_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VulkanEncoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        ctx.encoder_output_parameters_h264_low_latency(rate_control)
    }

    fn encoder_output_parameters_h265_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VulkanEncoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        ctx.encoder_output_parameters_h265_high_quality(rate_control)
    }

    fn encoder_output_parameters_h264_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VulkanEncoderError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        ctx.encoder_output_parameters_h264_high_quality(rate_control)
    }

    fn supports_decoding(&self) -> Result<bool, RegistryError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        Ok(ctx.supports_decoding())
    }

    fn supports_encoding(&self) -> Result<bool, RegistryError> {
        let ctx = GlobalRegistry::get_device(&self.into())?;
        Ok(ctx.supports_encoding())
    }
}

pub(crate) fn create_and_register_wgpu_device(
    video_device: Arc<VideoDevice>,
    wgpu_adapter: &wgpu::Adapter,
    desc: VideoDeviceDescriptor,
    required_extensions: &[&'static CStr],
    wgpu_queue_family_index: u32,
) -> Result<(wgpu::Device, wgpu::Queue), WgpuInitError> {
    let VideoDeviceDescriptor {
        wgpu_features,
        wgpu_experimental_features,
        wgpu_limits,
    } = desc;

    let wgpu_features = wgpu_features | wgpu::Features::TEXTURE_FORMAT_NV12;
    let device_key_for_dropping = Arc::new(OnceLock::new());
    let device_key_for_dropping_clone = device_key_for_dropping.clone();

    let hal_adapter = unsafe { wgpu_adapter.as_hal::<wgpu::hal::vulkan::Api>().unwrap() };
    let device_clone = video_device.device.clone();
    let wgpu_device = unsafe {
        hal_adapter.device_from_raw(
            device_clone.device.clone(),
            Some(Box::new(move || {
                match device_key_for_dropping_clone.get() {
                    Some(key) => GlobalRegistry::unregister_device(key),
                    None => {
                        tracing::debug!(
                            "Tried to drop device not registered in the global registry"
                        )
                    }
                }

                drop(device_clone);
            })),
            required_extensions,
            wgpu_features,
            &wgpu_limits,
            &wgpu::MemoryHints::default(),
            wgpu_queue_family_index,
            0,
        )?
    };

    let (wgpu_device, wgpu_queue) = unsafe {
        wgpu_adapter.create_device_from_hal(
            wgpu_device,
            &wgpu::DeviceDescriptor {
                label: Some("wgpu device created by the vulkan video decoder"),
                memory_hints: wgpu::MemoryHints::default(),
                required_limits: wgpu_limits,
                required_features: wgpu_features,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu_experimental_features,
            },
        )?
    };

    let device_key = VideoDeviceKey::from(&wgpu_device);
    device_key_for_dropping.set(device_key).unwrap();
    GlobalRegistry::register_device(device_key, video_device);

    Ok((wgpu_device, wgpu_queue))
}
