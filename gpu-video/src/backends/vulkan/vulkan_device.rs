use std::ffi::CStr;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;

use ash::vk;

use crate::backends::vulkan::codec::EncodeCodec;
use crate::backends::vulkan::codec::h264::H264Codec;
use crate::backends::vulkan::wrappers::*;
use crate::backends::vulkan::{VulkanAdapter, VulkanAdapterInfo};
use crate::capabilities::{DecodeCapabilities, EncodeCapabilities};
use crate::device::{
    ColorRange, CoreVideoDeviceBackend, DecoderParameters, EncoderOutputParameters,
    EncoderParametersH264, EncoderParametersH265, Rational, VideoDeviceDescriptor,
};
use crate::frame_sorter::FrameSorter;
use crate::parameters::EncoderPreset;
use crate::parser::h264::H264Parser;
use crate::parser::reference_manager::ReferenceContext;
use crate::vulkan_decoder::{ImageModifiers, VulkanDecoder};
use crate::vulkan_encoder::{FullEncoderParameters, VulkanEncoder};
use crate::{
    BytesDecoder, BytesEncoderH264, BytesEncoderH265, RawFrameData, VideoBackendError,
    VideoDecoderError, VideoDeviceInitError, VideoEncoderError, VulkanDecoderError,
};

use self::caps::{
    NativeDecodeCapabilities, NativeDecodeProfileCapabilities, NativeEncodeCapabilities,
};
use self::queues::{Queue, QueueIndex, Queues, VideoQueues};

#[cfg(feature = "wgpu")]
mod wgpu_api;

pub(crate) mod caps;
pub(crate) mod queues;

pub struct VulkanDevice {
    pub(crate) _physical_device: vk::PhysicalDevice,
    pub(crate) allocator: Arc<Allocator>,
    pub(crate) queues: Queues,
    pub(crate) native_decode_capabilities: Option<NativeDecodeCapabilities>,
    pub(crate) native_encode_capabilities: Option<NativeEncodeCapabilities>,
    pub(crate) adapter_info: Arc<VulkanAdapterInfo>,
    pub(crate) device: Arc<Device>,
}

impl CoreVideoDeviceBackend for VulkanDevice {
    fn create_bytes_decoder_h264(
        self: Arc<Self>,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, VideoDecoderError> {
        VulkanDevice::create_bytes_decoder_h264(self, parameters).map_err(Into::into)
    }

    fn create_bytes_encoder_h264(
        self: Arc<Self>,
        parameters: EncoderParametersH264,
    ) -> Result<BytesEncoderH264, VideoEncoderError> {
        VulkanDevice::create_bytes_encoder_h264(self, parameters)
    }

    fn create_bytes_encoder_h265(
        self: Arc<Self>,
        parameters: EncoderParametersH265,
    ) -> Result<BytesEncoderH265, VideoEncoderError> {
        VulkanDevice::create_bytes_encoder_h265(self, parameters)
    }

    #[cfg(feature = "transcoder")]
    fn create_transcoder(
        self: Arc<Self>,
        parameters: crate::parameters::TranscoderParameters,
    ) -> Result<crate::vulkan_transcoder::Transcoder, crate::vulkan_transcoder::VideoTranscoderError>
    {
        crate::vulkan_transcoder::Transcoder::new(self, parameters)
    }

    fn decode_capabilities(&self) -> DecodeCapabilities {
        self.adapter_info.decode_capabilities
    }

    fn encode_capabilities(&self) -> EncodeCapabilities {
        self.adapter_info.encode_capabilities
    }
}

impl VulkanDevice {
    pub(crate) fn create(
        video_adapter: VulkanAdapter<'_>,
        _desc: VideoDeviceDescriptor,
    ) -> Result<crate::VideoDevice, VulkanDeviceInitError> {
        let mut required_extensions = video_adapter.required_extensions();
        required_extensions.push(ash::khr::timeline_semaphore::NAME);

        let mut timeline_semaphore_feature =
            vk::PhysicalDeviceTimelineSemaphoreFeatures::default().timeline_semaphore(true);

        let mut device_create_info = vk::DeviceCreateInfo::default();
        device_create_info = device_create_info.push_next(&mut timeline_semaphore_feature);

        let video_device =
            Self::new_from_create_info(video_adapter, &required_extensions, device_create_info)?;

        Ok(crate::VideoDevice {
            inner: video_device,
            #[cfg(feature = "wgpu")]
            wgpu_device: None,
        })
    }

    fn new_from_create_info(
        adapter: VulkanAdapter<'_>,
        required_extensions: &[&'static CStr],
        device_create_info: vk::DeviceCreateInfo<'_>,
    ) -> Result<Arc<Self>, VulkanDeviceInitError> {
        let VulkanAdapter {
            instance,
            physical_device,
            queue_indices,
            decode_capabilities,
            encode_capabilities,
            info,
            ..
        } = adapter;

        let required_extensions_as_ptrs = required_extensions
            .iter()
            .map(|e| e.as_ptr())
            .collect::<Vec<_>>();

        let queue_create_infos = queue_indices.queue_create_infos();
        let queue_create_infos = queue_create_infos
            .iter()
            .map(|q| q.info())
            .collect::<Vec<_>>();

        let mut vk_synch_2_feature =
            vk::PhysicalDeviceSynchronization2Features::default().synchronization2(true);
        let mut vk_video_maintenance1_feature =
            vk::PhysicalDeviceVideoMaintenance1FeaturesKHR::default().video_maintenance1(true);
        let mut vk_descriptor_feature = vk::PhysicalDeviceDescriptorIndexingFeatures::default()
            .descriptor_binding_partially_bound(true);

        let device_create_info = device_create_info
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&required_extensions_as_ptrs)
            .push_next(&mut vk_synch_2_feature)
            .push_next(&mut vk_video_maintenance1_feature)
            .push_next(&mut vk_descriptor_feature);

        let device = unsafe {
            instance
                .instance
                .create_device(physical_device, &device_create_info, None)?
        };

        let video_queue_ext = ash::khr::video_queue::Device::new(&instance.instance, &device);
        let video_decode_queue_ext =
            ash::khr::video_decode_queue::Device::new(&instance.instance, &device);

        let video_encode_queue_ext =
            ash::khr::video_encode_queue::Device::new(&instance.instance, &device);
        let debug_utils_ext = instance
            .instance
            .debug_utils_instance_ext
            .as_ref()
            .map(|_| ash::ext::debug_utils::Device::new(&instance.instance, &device));

        let device = Arc::new(Device {
            device,
            video_queue_ext,
            video_decode_queue_ext,
            video_encode_queue_ext,
            debug_utils_ext,
            _instance: instance.instance.clone(),
        });

        let h264_decode_queues =
            queue_indices
                .h264_decode
                .as_ref()
                .map_or(Vec::new(), |queue_family_index| {
                    (0..queue_family_index.queue_count)
                        .map(|idx| queue_from_device(device.clone(), queue_family_index, idx))
                        .collect::<Vec<_>>()
                });
        let h264_encode_queues =
            queue_indices
                .encode
                .as_ref()
                .map_or(Vec::new(), |queue_family_index| {
                    (0..queue_family_index.queue_count)
                        .map(|idx| queue_from_device(device.clone(), queue_family_index, idx))
                        .collect::<Vec<_>>()
                });
        let transfer_queue = queue_from_device(device.clone(), &queue_indices.transfer, 0);
        let compute_queue =
            if queue_indices.compute.family_index == queue_indices.transfer.family_index {
                if queue_indices.transfer.queue_count > 1 {
                    queue_from_device(device.clone(), &queue_indices.transfer, 1)
                } else {
                    transfer_queue.clone()
                }
            } else {
                queue_from_device(device.clone(), &queue_indices.compute, 0)
            };
        let wgpu_queue =
            queue_from_device(device.clone(), &queue_indices.graphics_transfer_compute, 0);

        let queues = Queues {
            transfer: transfer_queue,
            compute: compute_queue,
            h264_decode: VideoQueues::new(h264_decode_queues.into_boxed_slice()).map(Arc::new),
            encode: VideoQueues::new(h264_encode_queues.into_boxed_slice()).map(Arc::new),
            wgpu: wgpu_queue,
        };

        let allocator = Arc::new(Allocator::new(
            instance.instance.clone(),
            physical_device,
            device.clone(),
        )?);

        Ok(Arc::new(Self {
            _physical_device: physical_device,
            device,
            allocator,
            queues,
            native_decode_capabilities: decode_capabilities,
            native_encode_capabilities: encode_capabilities,
            adapter_info: Arc::new(info),
        }))
    }

    pub fn create_bytes_decoder_h264(
        self: Arc<Self>,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, VulkanDecoderError> {
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
        let frame_sorter = FrameSorter::<RawFrameData>::new();

        Ok(BytesDecoder {
            parser,
            reference_ctx,
            decoder: Box::new(vulkan_decoder),
            frame_sorter,
        })
    }

    pub fn create_bytes_encoder_h264(
        self: Arc<Self>,
        parameters: EncoderParametersH264,
    ) -> Result<BytesEncoderH264, VideoEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;

        Ok(BytesEncoderH264 {
            vulkan_encoder: VulkanEncoder::new(Arc::new(self.encoding_device()?), parameters)?,
        })
    }

    pub fn create_bytes_encoder_h265(
        self: Arc<Self>,
        parameters: EncoderParametersH265,
    ) -> Result<BytesEncoderH265, VideoEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;

        Ok(BytesEncoderH265 {
            vulkan_encoder: VulkanEncoder::new(Arc::new(self.encoding_device()?), parameters)?,
        })
    }

    pub(crate) fn native_decode_capabilities(&self) -> Option<&NativeDecodeCapabilities> {
        self.native_decode_capabilities.as_ref()
    }

    pub(crate) fn native_encode_capabilities(&self) -> Option<&NativeEncodeCapabilities> {
        self.native_encode_capabilities.as_ref()
    }

    pub(crate) fn encoding_device(self: &Arc<Self>) -> Result<EncodingDevice, VideoEncoderError> {
        Ok(EncodingDevice {
            vulkan_device: self.clone(),
            encode_queues: self
                .queues
                .encode
                .clone()
                .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?,
            native_encode_capabilities: self
                .native_encode_capabilities
                .clone()
                .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?,
        })
    }

    pub(crate) fn decoding_device(self: &Arc<Self>) -> Result<DecodingDevice, VulkanDecoderError> {
        let decode_caps = self
            .native_decode_capabilities()
            .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?
            .h264
            .as_ref()
            .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?;

        let max_profile = decode_caps
            .max_profile()
            .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?;

        Ok(DecodingDevice {
            vulkan_device: self.clone(),
            h264_decode_queues: self
                .queues
                .h264_decode
                .clone()
                .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?,
            profile_capabilities: decode_caps
                .profile(max_profile)
                .cloned()
                .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?,
        })
    }

    pub(crate) fn validate_and_fill_encoder_parameters<C: EncodeCodec>(
        &self,
        encoder_parameters: EncoderOutputParameters<C::Profile>,
        width: NonZeroU32,
        height: NonZeroU32,
        framerate: Rational,
    ) -> Result<FullEncoderParameters<C>, VideoEncoderError> {
        let Some(caps) = self.native_encode_capabilities() else {
            return Err(VideoEncoderError::VulkanEncoderUnsupported);
        };
        let native_profile_caps =
            C::encode_codec_profile_capabilities(caps, encoder_parameters.profile)?;

        let (quality_level, tuning_mode) = match encoder_parameters.preset {
            EncoderPreset::HighQuality => (
                native_profile_caps
                    .quality_level_properties
                    .len()
                    .saturating_sub(1) as u32,
                vk::VideoEncodeTuningModeKHR::HIGH_QUALITY,
            ),
            EncoderPreset::Balanced => (
                native_profile_caps.quality_level_properties.len() as u32 / 2,
                vk::VideoEncodeTuningModeKHR::DEFAULT,
            ),
            EncoderPreset::LowLatency => (0, vk::VideoEncodeTuningModeKHR::LOW_LATENCY),
        };

        let native_quality_level_properties =
            &native_profile_caps.quality_level_properties[quality_level as usize];

        let idr_period = C::resolve_idr_period(
            &native_quality_level_properties.codec_quality_level_properties,
            encoder_parameters.idr_period,
        );

        let min_extent = native_profile_caps.video_capabilities.min_coded_extent;
        let max_extent = native_profile_caps.video_capabilities.max_coded_extent;

        if width.get() < min_extent.width || width.get() > max_extent.width {
            return Err(VideoEncoderError::ParametersError {
                field: "width",
                problem: format!(
                    "Width is {}, should be between {} and {}.",
                    width, min_extent.width, max_extent.width
                ),
            });
        }

        if height.get() < min_extent.height || height.get() > max_extent.height {
            return Err(VideoEncoderError::ParametersError {
                field: "height",
                problem: format!(
                    "Height is {}, should be between {} and {}.",
                    height, min_extent.height, max_extent.height
                ),
            });
        }

        let rate_control = encoder_parameters.rate_control;
        if !native_profile_caps
            .encode_capabilities
            .rate_control_modes
            .contains(rate_control.to_vk())
        {
            return Err(VideoEncoderError::ParametersError {
                field: "rate_control",
                problem: format!(
                    "Rate control has mode {:?}. Supported modes are: {:?}.",
                    rate_control.to_vk(),
                    native_profile_caps.encode_capabilities.rate_control_modes
                ),
            });
        }

        let max_references = C::resolve_max_references(
            &native_quality_level_properties.codec_quality_level_properties,
            &native_profile_caps.codec_encode_capabilities,
            encoder_parameters.max_references,
        );

        if framerate.numerator.checked_mul(2).is_none() {
            return Err(VideoEncoderError::ParametersError {
                field: "framerate",
                problem: "Framerate numerator * 2 must fit in u32".to_string(),
            });
        }
        if framerate.numerator == 0 {
            return Err(VideoEncoderError::ParametersError {
                field: "framerate",
                problem: format!("Framerate is {framerate:?}. The numerator should be != 0.",),
            });
        }
        let usage_flags = encoder_parameters.usage_flags.unwrap_or_default().into();
        let content_flags = vk::VideoEncodeContentFlagsKHR::DEFAULT;
        let color_space = encoder_parameters.color_space.unwrap_or_default();
        let color_range = encoder_parameters
            .color_range
            .unwrap_or(ColorRange::Limited);

        Ok(FullEncoderParameters {
            idr_period,
            width,
            height,
            rate_control,
            max_references,
            quality_level,
            profile: encoder_parameters.profile,
            framerate,
            usage_flags,
            tuning_mode,
            content_flags,
            inline_stream_params: encoder_parameters.inline_stream_params.unwrap_or(true),
            color_space,
            color_range,
        })
    }
}

impl std::fmt::Debug for VulkanDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDevice").finish()
    }
}

pub(crate) struct DecodingDevice {
    pub(crate) vulkan_device: Arc<VulkanDevice>,
    pub(crate) h264_decode_queues: Arc<VideoQueues>,
    pub(crate) profile_capabilities: NativeDecodeProfileCapabilities<H264Codec>,
}

impl Deref for DecodingDevice {
    type Target = VulkanDevice;

    fn deref(&self) -> &Self::Target {
        &self.vulkan_device
    }
}

pub(crate) struct EncodingDevice {
    pub(crate) vulkan_device: Arc<VulkanDevice>,
    pub(crate) encode_queues: Arc<VideoQueues>,
    pub(crate) native_encode_capabilities: NativeEncodeCapabilities,
}

impl Deref for EncodingDevice {
    type Target = VulkanDevice;

    fn deref(&self) -> &Self::Target {
        &self.vulkan_device
    }
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanDeviceInitError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    WgpuError(#[from] crate::WgpuInitError),
}

impl From<VulkanDeviceInitError> for VideoDeviceInitError {
    fn from(err: VulkanDeviceInitError) -> Self {
        match err {
            VulkanDeviceInitError::VkError(_) => Self::BackendError(VideoBackendError {
                message: err.to_string(),
                source: Box::new(err),
            }),
            #[cfg(feature = "wgpu")]
            VulkanDeviceInitError::WgpuError(_) => Self::BackendError(VideoBackendError {
                message: err.to_string(),
                source: Box::new(err),
            }),
        }
    }
}

fn queue_from_device(
    device: Arc<Device>,
    queue_family_index: &QueueIndex<'static>,
    queue_index: usize,
) -> Queue {
    let queue = unsafe {
        device.get_device_queue(queue_family_index.family_index as u32, queue_index as u32)
    };
    Queue {
        queue: Arc::new(queue.into()),
        family_index: queue_family_index.family_index,
        _video_properties: queue_family_index.video_properties,
        query_result_status_properties: queue_family_index.query_result_status_properties,
        device,
    }
}
