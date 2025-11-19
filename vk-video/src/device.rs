use std::ffi::CStr;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;

use ash::vk;

use crate::adapter::VulkanAdapter;
use crate::capabilities::AdapterInfo;
use crate::device::caps::{
    DecodeCapabilities, EncodeCapabilities, NativeDecodeCapabilities,
    NativeDecodeProfileCapabilities, NativeEncodeCapabilities,
};
use crate::device::queues::{Queue, Queues};
use crate::parameters::{
    EncoderContentFlags, EncoderTuningMode, EncoderUsageFlags, H264Profile, RateControl,
};
use crate::parser::{h264::H264Parser, reference_manager::ReferenceContext};
use crate::vulkan_decoder::{FrameSorter, VulkanDecoder};
use crate::vulkan_encoder::{FullEncoderParameters, VulkanEncoder};
use crate::{
    BytesDecoder, BytesEncoder, DecoderError, RawFrameData, VulkanDecoderError, VulkanEncoderError,
    VulkanInitError, VulkanInstance, WgpuTexturesDecoder, WgpuTexturesEncoder, wrappers::*,
};

pub(crate) mod caps;
pub(crate) mod queues;

pub(crate) const REQUIRED_EXTENSIONS: &[&CStr] = &[vk::KHR_VIDEO_QUEUE_NAME];

pub(crate) const DECODE_EXTENSIONS: &[&CStr] = &[
    vk::KHR_VIDEO_DECODE_QUEUE_NAME,
    vk::KHR_VIDEO_DECODE_H264_NAME,
];

pub(crate) const ENCODE_EXTENSIONS: &[&CStr] = &[
    vk::KHR_VIDEO_ENCODE_QUEUE_NAME,
    vk::KHR_VIDEO_ENCODE_H264_NAME,
];

/// A fraction
#[derive(Debug, Clone, Copy)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: NonZeroU32,
}

impl From<u32> for Rational {
    fn from(value: u32) -> Self {
        Rational {
            numerator: value,
            denominator: std::num::NonZeroU32::new(1).unwrap(),
        }
    }
}

/// An enum used to specify how the decoder should handle missing frames
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissedFrameHandling {
    /// When missed frames are detected, error on every subsequent frame that depends on them
    /// (i. e. fail on every frame until an IDR frame arrives)
    #[default]
    Strict,

    /// When missed frames are detected, try to decode later frames that depend on them anyway.
    /// This can produce decoded frames with very visible artifacts.
    Tolerant,
}

/// Parameters for decoder creation
#[derive(Debug, Default, Clone, Copy)]
pub struct DecoderParameters {
    /// See [`MissedFrameHandling`] for description of different handling approaches.
    ///
    /// **Defaults to [`MissedFrameHandling::Strict`]**
    pub missed_frame_handling: MissedFrameHandling,

    /// A hint indicating what kind of content the decoder is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub usage_flags: crate::parameters::DecoderUsageFlags,
}

/// Things the encoder needs to know about the video
#[derive(Debug, Clone, Copy)]
pub struct VideoParameters {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    /// The expected/approximate framerate of the encoded video
    pub target_framerate: Rational,
}

/// Parameters for encoder creation
#[derive(Debug, Clone, Copy)]
pub struct EncoderParameters {
    /// Number of frames between IDRs. If [`None`], this will be set to an encoder preferred value,
    /// or, if the encoder doesn't provide a preferred value, to 30.
    pub idr_period: Option<NonZeroU32>,
    /// See [`RateControl`] for description of different rate control modes. The selected mode must
    /// be supported by the device.
    pub rate_control: RateControl,
    /// Max number of references a P-frame can have. If [`None`], this value will be set
    /// to the max value supported by the device.
    pub max_references: Option<NonZeroU32>,
    /// The profile must be supported by the device
    pub profile: H264Profile,
    /// The value must be less than
    /// [`EncodeH264ProfileCapabilities::quality_levels`](crate::capabilities::EncodeH264ProfileCapabilities::quality_levels)
    pub quality_level: u32,
    pub video_parameters: VideoParameters,

    /// A hint indicating what the encoded content is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub usage_flags: Option<EncoderUsageFlags>,

    /// A hint indicating how to tune the encoder implementation.
    pub tuning_mode: Option<EncoderTuningMode>,

    /// A hint indicating what kind of content the encoder is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub content_flags: Option<EncoderContentFlags>,
}

/// Open connection to a coding-capable device. Also contains a [`wgpu::Device`], a [`wgpu::Queue`] and
/// a [`wgpu::Adapter`].
pub struct VulkanDevice {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) wgpu_adapter: wgpu::Adapter,
    pub(crate) _physical_device: vk::PhysicalDevice,
    pub(crate) allocator: Arc<Allocator>,
    pub(crate) queues: Queues,
    pub(crate) native_decode_capabilities: Option<NativeDecodeCapabilities>,
    pub(crate) native_encode_capabilities: Option<NativeEncodeCapabilities>,
    pub(crate) adapter_info: AdapterInfo,
    pub(crate) device: Arc<Device>,
}

impl VulkanDevice {
    pub(crate) fn new(
        instance: &VulkanInstance,
        wgpu_features: wgpu::Features,
        wgpu_limits: wgpu::Limits,
        adapter: VulkanAdapter<'_>,
    ) -> Result<Self, VulkanInitError> {
        let VulkanAdapter {
            physical_device,
            wgpu_adapter,
            queue_indices,
            decode_capabilities,
            encode_capabilities,
            info,
            ..
        } = adapter;

        let wgpu_features = wgpu_features | wgpu::Features::TEXTURE_FORMAT_NV12;
        let wgpu_extensions = wgpu_adapter
            .adapter
            .required_device_extensions(wgpu_features);

        let required_extensions = REQUIRED_EXTENSIONS
            .iter()
            .copied()
            .chain(wgpu_extensions)
            .chain(match info.supports_decoding {
                true => DECODE_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .chain(match info.supports_encoding {
                true => ENCODE_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .collect::<Vec<_>>();

        let required_extensions_as_ptrs = required_extensions
            .iter()
            .map(|e| e.as_ptr())
            .collect::<Vec<_>>();

        let queue_create_infos = queue_indices.queue_create_infos();

        let mut wgpu_physical_device_features = wgpu_adapter
            .adapter
            .physical_device_features(&required_extensions, wgpu_features);

        let mut vk_synch_2_feature =
            vk::PhysicalDeviceSynchronization2Features::default().synchronization2(true);

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&required_extensions_as_ptrs);

        let device_create_info = wgpu_physical_device_features
            .add_to_device_create(device_create_info)
            .push_next(&mut vk_synch_2_feature);

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

        let device = Arc::new(Device {
            device,
            video_queue_ext,
            video_decode_queue_ext,
            video_encode_queue_ext,
            _instance: instance.instance.clone(),
        });

        let h264_decode_queue = queue_indices.h264_decode.map(|queue_index| {
            (
                unsafe { device.get_device_queue(queue_index.idx as u32, 0) },
                queue_index,
            )
        });
        let h264_encode_queue = queue_indices.h264_encode.map(|queue_index| {
            (
                unsafe { device.get_device_queue(queue_index.idx as u32, 0) },
                queue_index,
            )
        });
        let transfer_queue =
            unsafe { device.get_device_queue(queue_indices.transfer.idx as u32, 0) };
        let wgpu_queue = unsafe {
            device.get_device_queue(queue_indices.graphics_transfer_compute.idx as u32, 0)
        };

        let queues = Queues {
            transfer: Queue {
                queue: Arc::new(transfer_queue.into()),
                idx: queue_indices.transfer.idx,
                _video_properties: queue_indices.transfer.video_properties,
                query_result_status_properties: queue_indices
                    .transfer
                    .query_result_status_properties,
                device: device.clone(),
            },
            h264_decode: h264_decode_queue.map(|(queue, queue_index)| Queue {
                queue: Arc::new(queue.into()),
                idx: queue_index.idx,
                _video_properties: queue_index.video_properties,
                query_result_status_properties: queue_index.query_result_status_properties,
                device: device.clone(),
            }),
            h264_encode: h264_encode_queue.map(|(queue, queue_index)| Queue {
                queue: Arc::new(queue.into()),
                idx: queue_index.idx,
                _video_properties: queue_index.video_properties,
                query_result_status_properties: queue_index.query_result_status_properties,
                device: device.clone(),
            }),
            wgpu: Queue {
                queue: Arc::new(wgpu_queue.into()),
                idx: queue_indices.graphics_transfer_compute.idx,
                _video_properties: queue_indices.graphics_transfer_compute.video_properties,
                query_result_status_properties: queue_indices
                    .graphics_transfer_compute
                    .query_result_status_properties,
                device: device.clone(),
            },
        };

        let device_clone = device.clone();

        let wgpu_device = unsafe {
            wgpu_adapter.adapter.device_from_raw(
                device.device.clone(),
                Some(Box::new(move || {
                    drop(device_clone);
                })),
                &required_extensions,
                wgpu_features,
                &wgpu::MemoryHints::default(),
                queue_indices.graphics_transfer_compute.idx as u32,
                0,
            )?
        };

        let allocator = Arc::new(Allocator::new(
            instance.instance.clone(),
            physical_device,
            device.clone(),
        )?);

        let wgpu_adapter = unsafe { instance.wgpu_instance.create_adapter_from_hal(wgpu_adapter) };
        let (wgpu_device, wgpu_queue) = unsafe {
            wgpu_adapter.create_device_from_hal(
                wgpu_device,
                &wgpu::DeviceDescriptor {
                    label: Some("wgpu device created by the vulkan video decoder"),
                    memory_hints: wgpu::MemoryHints::default(),
                    required_limits: wgpu_limits,
                    required_features: wgpu_features,
                    trace: wgpu::Trace::Off,
                },
            )?
        };

        Ok(VulkanDevice {
            _physical_device: physical_device,
            device,
            allocator,
            queues,
            native_decode_capabilities: decode_capabilities,
            native_encode_capabilities: encode_capabilities,
            wgpu_device,
            wgpu_queue,
            wgpu_adapter,
            adapter_info: info,
        })
    }

    pub fn create_wgpu_textures_decoder(
        self: &Arc<Self>,
        parameters: DecoderParameters,
    ) -> Result<WgpuTexturesDecoder, DecoderError> {
        let decode_caps = self
            .native_decode_capabilities
            .as_ref()
            .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?;
        let max_profile = decode_caps.max_profile();

        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::new(parameters.missed_frame_handling);
        let decoding_device = DecodingDevice {
            vulkan_device: self.clone(),
            h264_decode_queue: self
                .queues
                .h264_decode
                .clone()
                .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?,
            profile_capabilities: decode_caps
                .profile(max_profile)
                .cloned()
                .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?,
        };

        let vulkan_decoder = VulkanDecoder::new(Arc::new(decoding_device), parameters.usage_flags)?;
        let frame_sorter = FrameSorter::<wgpu::Texture>::new();

        Ok(WgpuTexturesDecoder {
            parser,
            reference_ctx,
            vulkan_decoder,
            frame_sorter,
        })
    }

    pub fn create_bytes_decoder(
        self: &Arc<Self>,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, DecoderError> {
        let decode_caps = self
            .native_decode_capabilities
            .as_ref()
            .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?;
        let max_profile = decode_caps.max_profile();

        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::new(parameters.missed_frame_handling);
        let decoding_device = DecodingDevice {
            vulkan_device: self.clone(),
            h264_decode_queue: self
                .queues
                .h264_decode
                .clone()
                .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?,
            profile_capabilities: decode_caps
                .profile(max_profile)
                .cloned()
                .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?,
        };

        let vulkan_decoder = VulkanDecoder::new(Arc::new(decoding_device), parameters.usage_flags)?;
        let frame_sorter = FrameSorter::<RawFrameData>::new();

        Ok(BytesDecoder {
            parser,
            reference_ctx,
            vulkan_decoder,
            frame_sorter,
        })
    }

    pub fn wgpu_device(&self) -> wgpu::Device {
        self.wgpu_device.clone()
    }

    pub fn wgpu_queue(&self) -> wgpu::Queue {
        self.wgpu_queue.clone()
    }

    pub fn wgpu_adapter(&self) -> wgpu::Adapter {
        self.wgpu_adapter.clone()
    }

    pub fn create_bytes_encoder(
        self: &Arc<Self>,
        parameters: EncoderParameters,
    ) -> Result<BytesEncoder, VulkanEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(parameters)?;
        let encoding_device = EncodingDevice {
            vulkan_device: self.clone(),
            h264_encode_queue: self
                .queues
                .h264_encode
                .clone()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            native_encode_capabilities: self
                .native_encode_capabilities
                .clone()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
        };
        let encoder = VulkanEncoder::new(Arc::new(encoding_device), parameters)?;
        Ok(BytesEncoder {
            vulkan_encoder: encoder,
        })
    }

    pub fn create_wgpu_textures_encoder(
        self: &Arc<Self>,
        parameters: EncoderParameters,
    ) -> Result<WgpuTexturesEncoder, VulkanEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(parameters)?;
        let encoding_device = EncodingDevice {
            vulkan_device: self.clone(),
            h264_encode_queue: self
                .queues
                .h264_encode
                .clone()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            native_encode_capabilities: self
                .native_encode_capabilities
                .clone()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
        };
        let encoder = VulkanEncoder::new_with_converter(Arc::new(encoding_device), parameters)?;
        Ok(WgpuTexturesEncoder {
            vulkan_encoder: encoder,
        })
    }

    pub fn decode_capabilities(&self) -> DecodeCapabilities {
        self.adapter_info.decode_capabilities
    }

    pub fn encode_capabilities(&self) -> EncodeCapabilities {
        self.adapter_info.encode_capabilities
    }

    pub fn encoder_parameters_low_latency(
        &self,
        video_parameters: VideoParameters,
        rate_control: RateControl,
    ) -> Result<EncoderParameters, VulkanEncoderError> {
        let Some(caps) = self.native_encode_capabilities.as_ref() else {
            return Err(VulkanEncoderError::VulkanEncoderUnsupported);
        };

        Ok(EncoderParameters {
            video_parameters,
            profile: caps.max_profile(),
            idr_period: None,
            max_references: None,
            rate_control,
            quality_level: 0,
            usage_flags: Some(EncoderUsageFlags::DEFAULT),
            content_flags: Some(EncoderContentFlags::DEFAULT),
            tuning_mode: Some(EncoderTuningMode::LOW_LATENCY),
        })
    }

    pub fn encoder_parameters_high_quality(
        &self,
        video_parameters: VideoParameters,
        rate_control: RateControl,
    ) -> Result<EncoderParameters, VulkanEncoderError> {
        let Some(caps) = self.native_encode_capabilities.as_ref() else {
            return Err(VulkanEncoderError::VulkanEncoderUnsupported);
        };

        Ok(EncoderParameters {
            video_parameters,
            profile: caps.max_profile(),
            idr_period: None,
            max_references: None,
            rate_control,
            quality_level: caps
                .profile(caps.max_profile())
                .unwrap()
                .encode_capabilities
                .max_quality_levels
                - 1,
            usage_flags: Some(EncoderUsageFlags::DEFAULT),
            content_flags: Some(EncoderContentFlags::DEFAULT),
            tuning_mode: Some(EncoderTuningMode::HIGH_QUALITY),
        })
    }

    fn validate_and_fill_encoder_parameters(
        &self,
        encoder_parameters: EncoderParameters,
    ) -> Result<FullEncoderParameters, VulkanEncoderError> {
        let Some(caps) = self.native_encode_capabilities.as_ref() else {
            return Err(VulkanEncoderError::VulkanEncoderUnsupported);
        };
        let native_profile_caps = caps.profile(encoder_parameters.profile).ok_or(
            VulkanEncoderError::ParametersError {
                field: "profile",
                problem: format!(
                    "Profile {:?} is not supported by this device.",
                    encoder_parameters.profile
                ),
            },
        )?;

        let native_quality_level_properties = native_profile_caps
            .quality_level_properties
            .get(encoder_parameters.quality_level as usize)
            .ok_or(VulkanEncoderError::ParametersError {
                field: "quality_level",
                problem: format!(
                    "Quality level is {}, should be < {}",
                    encoder_parameters.quality_level,
                    native_profile_caps.quality_level_properties.len()
                ),
            })?;

        let idr_period = encoder_parameters.idr_period.unwrap_or(
            if native_quality_level_properties
                .h264_quality_level_properties
                .preferred_idr_period
                > 0
            {
                NonZeroU32::new(
                    native_quality_level_properties
                        .h264_quality_level_properties
                        .preferred_idr_period,
                )
                .unwrap()
            } else {
                NonZeroU32::new(30).unwrap()
            },
        );

        let min_extent = native_profile_caps.video_capabilities.min_coded_extent;
        let max_extent = native_profile_caps.video_capabilities.max_coded_extent;

        let width = encoder_parameters.video_parameters.width;
        if width.get() < min_extent.width || width.get() > max_extent.width {
            return Err(VulkanEncoderError::ParametersError {
                field: "width",
                problem: format!(
                    "Width is {}, should be between {} and {}.",
                    width, min_extent.width, max_extent.width
                ),
            });
        }

        let height = encoder_parameters.video_parameters.height;
        if height.get() < min_extent.height || height.get() > max_extent.height {
            return Err(VulkanEncoderError::ParametersError {
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
            return Err(VulkanEncoderError::ParametersError {
                field: "rate_control",
                problem: format!(
                    "Rate control has mode {:?}. Supported modes are: {:?}.",
                    rate_control.to_vk(),
                    native_profile_caps.encode_capabilities.rate_control_modes
                ),
            });
        }

        let max_references = encoder_parameters.max_references.unwrap_or(
            if native_quality_level_properties
                .h264_quality_level_properties
                .preferred_max_l0_reference_count
                > 0
            {
                NonZeroU32::new(
                    native_quality_level_properties
                        .h264_quality_level_properties
                        .preferred_max_l0_reference_count,
                )
                .unwrap()
            } else {
                NonZeroU32::new(
                    native_profile_caps
                        .h264_encode_capabilities
                        .max_p_picture_l0_reference_count,
                )
                .unwrap()
            },
        );

        if max_references.get()
            > native_profile_caps
                .h264_encode_capabilities
                .max_p_picture_l0_reference_count
        {
            return Err(VulkanEncoderError::ParametersError {
                field: "max_references",
                problem: format!(
                    "Max references is {}, should be != 0 and <= {}",
                    max_references,
                    native_profile_caps
                        .h264_encode_capabilities
                        .max_p_picture_l0_reference_count
                ),
            });
        }

        let framerate = encoder_parameters.video_parameters.target_framerate;
        if framerate.numerator == 0 {
            return Err(VulkanEncoderError::ParametersError {
                field: "framerate",
                problem: format!("Framerate is {framerate:?}. The numerator should be != 0.",),
            });
        }

        let usage_flags = encoder_parameters
            .usage_flags
            .unwrap_or(vk::VideoEncodeUsageFlagsKHR::DEFAULT);
        let tuning_mode = encoder_parameters
            .tuning_mode
            .unwrap_or(vk::VideoEncodeTuningModeKHR::DEFAULT);
        let content_flags = encoder_parameters
            .content_flags
            .unwrap_or(vk::VideoEncodeContentFlagsKHR::DEFAULT);

        Ok(FullEncoderParameters {
            idr_period,
            width,
            height,
            rate_control,
            max_references,
            quality_level: encoder_parameters.quality_level,
            profile: encoder_parameters.profile,
            framerate,
            usage_flags,
            tuning_mode,
            content_flags,
        })
    }

    pub fn supports_decoding(&self) -> bool {
        self.adapter_info.supports_decoding
    }

    pub fn supports_encoding(&self) -> bool {
        self.adapter_info.supports_encoding
    }
}

impl std::fmt::Debug for VulkanDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDevice").finish()
    }
}

pub(crate) struct DecodingDevice {
    pub(crate) vulkan_device: Arc<VulkanDevice>,
    pub(crate) h264_decode_queue: Queue,
    pub(crate) profile_capabilities: NativeDecodeProfileCapabilities,
}

impl Deref for DecodingDevice {
    type Target = VulkanDevice;

    fn deref(&self) -> &Self::Target {
        &self.vulkan_device
    }
}

pub(crate) struct EncodingDevice {
    pub(crate) vulkan_device: Arc<VulkanDevice>,
    pub(crate) h264_encode_queue: Queue,
    pub(crate) native_encode_capabilities: NativeEncodeCapabilities,
}

impl Deref for EncodingDevice {
    type Target = VulkanDevice;

    fn deref(&self) -> &Self::Target {
        &self.vulkan_device
    }
}
