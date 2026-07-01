use std::sync::{Arc, OnceLock};

use ash::vk;
use wgpu::hal::vulkan::Api as VkApi;

use crate::{
    VideoDecoderError, VideoEncoderError, VulkanDecoderError, WgpuInitError, WgpuTexturesDecoder,
    backends::{
        WgpuBackend,
        vulkan::{VulkanAdapter, VulkanBackend, VulkanDevice, VulkanDeviceInitError},
    },
    device::{
        DecoderParameters, EncoderParametersH264, EncoderParametersH265, VideoDeviceDescriptor,
        WgpuVideoDeviceBackend,
    },
    frame_sorter::FrameSorter,
    global_registry::GlobalRegistry,
    parser::{h264::H264Parser, reference_manager::ReferenceContext},
    vulkan_decoder::{ImageModifiers, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
};

impl WgpuVideoDeviceBackend for VulkanDevice {
    fn create_wgpu_textures_decoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        parameters: DecoderParameters,
    ) -> Result<crate::WgpuTexturesDecoder, VideoDecoderError> {
        VulkanDevice::create_wgpu_textures_decoder_h264(self, wgpu_device, parameters)
            .map_err(Into::into)
    }

    fn create_wgpu_textures_encoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, VideoEncoderError> {
        VulkanDevice::create_wgpu_textures_encoder_h264(self, wgpu_device, wgpu_queue, parameters)
    }

    fn create_wgpu_textures_encoder_h265(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, VideoEncoderError> {
        VulkanDevice::create_wgpu_textures_encoder_h265(self, wgpu_device, wgpu_queue, parameters)
    }
}

impl VulkanDevice {
    pub(crate) fn create_and_register_wgpu(
        wgpu_adapter: &wgpu::Adapter,
        video_adapter: VulkanAdapter<'_>,
        desc: VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), VulkanDeviceInitError> {
        let hal_adapter = unsafe { wgpu_adapter.as_hal::<VkApi>().unwrap() };

        let wgpu_queue_family_index = video_adapter
            .queue_indices
            .graphics_transfer_compute
            .family_index as u32;
        let mut required_extensions = video_adapter.required_extensions();

        let wgpu_features = desc.wgpu_features | wgpu::Features::TEXTURE_FORMAT_NV12;
        let mut wgpu_extensions = hal_adapter.required_device_extensions(wgpu_features);
        required_extensions.append(&mut wgpu_extensions);

        let mut wgpu_physical_device_features = unsafe {
            wgpu_adapter
                .as_hal::<wgpu::hal::vulkan::Api>()
                .unwrap()
                .physical_device_features(&required_extensions, desc.wgpu_features)
        };

        let mut device_create_info = vk::DeviceCreateInfo::default();
        device_create_info = wgpu_physical_device_features.add_to_device_create(device_create_info);

        let video_device =
            Self::new_from_create_info(video_adapter, &required_extensions, device_create_info)?;

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
            hal_adapter
                .device_from_raw(
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
                    &required_extensions,
                    wgpu_features,
                    &wgpu_limits,
                    &wgpu::MemoryHints::default(),
                    wgpu_queue_family_index,
                    0,
                )
                .map_err(WgpuInitError::WgpuDeviceError)?
        };

        let (wgpu_device, wgpu_queue) = unsafe {
            wgpu_adapter
                .create_device_from_hal(
                    wgpu_device,
                    &wgpu::DeviceDescriptor {
                        label: Some("wgpu device created by the vulkan video decoder"),
                        memory_hints: wgpu::MemoryHints::default(),
                        required_limits: wgpu_limits,
                        required_features: wgpu_features,
                        trace: wgpu::Trace::Off,
                        experimental_features: wgpu_experimental_features,
                    },
                )
                .map_err(WgpuInitError::WgpuRequestDeviceError)?
        };

        let device_key = VulkanBackend.device_key_from_wgpu_device(&wgpu_device);
        device_key_for_dropping.set(device_key).unwrap();
        GlobalRegistry::register_device(device_key, video_device);

        Ok((wgpu_device, wgpu_queue))
    }

    pub fn create_wgpu_textures_decoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        parameters: DecoderParameters,
    ) -> Result<WgpuTexturesDecoder, VulkanDecoderError> {
        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::new(parameters.missed_frame_handling);

        let vulkan_decoder = VulkanDecoder::new(
            Arc::new(self.decoding_device()?),
            parameters.usage_flags,
            ImageModifiers {
                additional_queue_index: self.queues.transfer.family_index,
                create_flags: Default::default(),
                usage_flags: Default::default(),
            },
        )?;
        let frame_sorter = FrameSorter::<wgpu::Texture>::new();

        Ok(crate::WgpuTexturesDecoder {
            wgpu_device,
            parser,
            reference_ctx,
            decoder: Box::new(vulkan_decoder),
            frame_sorter,
        })
    }

    pub fn create_wgpu_textures_encoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, VideoEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;

        Ok(crate::WgpuTexturesEncoderH264 {
            wgpu_device,
            wgpu_queue,
            vulkan_encoder: VulkanEncoder::new(Arc::new(self.encoding_device()?), parameters)?,
        })
    }

    pub fn create_wgpu_textures_encoder_h265(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, VideoEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;

        Ok(crate::WgpuTexturesEncoderH265 {
            wgpu_device,
            wgpu_queue,
            vulkan_encoder: VulkanEncoder::new(Arc::new(self.encoding_device()?), parameters)?,
        })
    }
}
