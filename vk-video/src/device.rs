use std::ffi::CStr;
use std::num::NonZeroU32;
use std::sync::Arc;

use ash::vk;
use tracing::{debug, warn};
use wgpu::hal::Adapter;

use crate::device::caps::{DecodeCapabilities, EncodeCapabilities, NativeEncodeCapabilities};
use crate::device::queues::{Queue, QueueIndex, QueueIndices, Queues};
use crate::parser::Parser;
use crate::vulkan_decoder::{FrameSorter, VulkanDecoder};
use crate::vulkan_encoder::{FullEncoderParameters, VulkanEncoder};
use crate::{
    wrappers::*, BytesDecoder, BytesEncoder, DecoderError, H264Profile, RateControl, RawFrameData,
    VulkanEncoderError, VulkanInitError, VulkanInstance, WgpuTexturesDecoder, WgpuTexturesEncoder,
};

pub(crate) mod caps;
pub(crate) mod queues;

pub(crate) const REQUIRED_EXTENSIONS: &[&CStr] = &[
    vk::KHR_VIDEO_QUEUE_NAME,
    vk::KHR_VIDEO_DECODE_QUEUE_NAME,
    vk::KHR_VIDEO_DECODE_H264_NAME,
    // TODO: We need a better mechanism for device selection, with feedback about which devices
    // support which operations. Some configurations might only have support for encode or decode,
    // and the user should be able to choose which one they want.
    vk::KHR_VIDEO_ENCODE_QUEUE_NAME,
    vk::KHR_VIDEO_ENCODE_H264_NAME,
];

/// A fraction
#[derive(Debug, Clone, Copy)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: NonZeroU32,
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
    /// Number of frames between IDRs. If [`None`], this will be set to 30.
    pub idr_period: Option<NonZeroU32>,
    /// See [`RateControl`] for description of differnt rate control modes. The selected mode must
    /// be supported by the device.
    pub rate_control: RateControl,
    /// Max number of references a P-frame can have. If [`None`], this value will be set
    /// to the max value supported by the device.
    pub max_references: Option<NonZeroU32>,
    /// The profile must be supported by the device
    pub profile: H264Profile,
    /// The value must be less than
    /// [`EncodeH264ProfileCapabilities::quality_levels`](crate::EncodeH264ProfileCapabilities::quality_levels)
    pub quality_level: u32,
    pub video_parameters: VideoParameters,
}

/// Open connection to a coding-capable device. Also contains a [`wgpu::Device`], a [`wgpu::Queue`] and
/// a [`wgpu::Adapter`].
pub struct VulkanDevice {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) wgpu_adapter: wgpu::Adapter,
    pub(crate) _physical_device: vk::PhysicalDevice,
    pub(crate) device: Arc<Device>,
    pub(crate) allocator: Arc<Allocator>,
    pub(crate) queues: Queues,
    pub(crate) decode_capabilities: DecodeCapabilities,
    pub(crate) native_encode_capabilities: NativeEncodeCapabilities,
}

impl VulkanDevice {
    pub(crate) fn new(
        instance: &VulkanInstance,
        wgpu_features: wgpu::Features,
        wgpu_limits: wgpu::Limits,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, VulkanInitError> {
        let physical_devices = unsafe { instance.instance.enumerate_physical_devices()? };

        let ChosenDevice {
            physical_device,
            wgpu_adapter,
            queue_indices,
            decode_capabilities,
            encode_capabilities,
        } = find_device(
            &physical_devices,
            &instance.instance,
            &instance.wgpu_instance,
            REQUIRED_EXTENSIONS,
            compatible_surface,
        )?;

        let wgpu_features = wgpu_features | wgpu::Features::TEXTURE_FORMAT_NV12;

        let wgpu_extensions = wgpu_adapter
            .adapter
            .required_device_extensions(wgpu_features);

        let required_extensions = REQUIRED_EXTENSIONS
            .iter()
            .copied()
            .chain(wgpu_extensions)
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

        let h264_decode_queue =
            unsafe { device.get_device_queue(queue_indices.h264_decode.idx as u32, 0) };
        let h264_encode_queue =
            unsafe { device.get_device_queue(queue_indices.h264_encode.idx as u32, 0) };
        let transfer_queue =
            unsafe { device.get_device_queue(queue_indices.transfer.idx as u32, 0) };
        let wgpu_queue = unsafe {
            device.get_device_queue(queue_indices.graphics_transfer_compute.idx as u32, 0)
        };

        let queues = Queues {
            transfer: Queue {
                queue: transfer_queue.into(),
                idx: queue_indices.transfer.idx,
                _video_properties: queue_indices.transfer.video_properties,
                query_result_status_properties: queue_indices
                    .transfer
                    .query_result_status_properties,
                device: device.clone(),
            },
            h264_decode: Queue {
                queue: h264_decode_queue.into(),
                idx: queue_indices.h264_decode.idx,
                _video_properties: queue_indices.h264_decode.video_properties,
                query_result_status_properties: queue_indices
                    .h264_decode
                    .query_result_status_properties,
                device: device.clone(),
            },
            h264_encode: Queue {
                queue: h264_encode_queue.into(),
                idx: queue_indices.h264_encode.idx,
                _video_properties: queue_indices.h264_encode.video_properties,
                query_result_status_properties: queue_indices
                    .h264_encode
                    .query_result_status_properties,
                device: device.clone(),
            },
            wgpu: Queue {
                queue: wgpu_queue.into(),
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
            decode_capabilities,
            native_encode_capabilities: encode_capabilities,
            wgpu_device,
            wgpu_queue,
            wgpu_adapter,
        })
    }

    pub fn create_wgpu_textures_decoder(
        self: &Arc<Self>,
    ) -> Result<WgpuTexturesDecoder, DecoderError> {
        let parser = Parser::default();
        let vulkan_decoder = VulkanDecoder::new(self.clone())?;
        let frame_sorter = FrameSorter::<wgpu::Texture>::new();

        Ok(WgpuTexturesDecoder {
            parser,
            vulkan_decoder,
            frame_sorter,
        })
    }

    pub fn create_bytes_decoder(self: &Arc<Self>) -> Result<BytesDecoder, DecoderError> {
        let parser = Parser::default();
        let vulkan_decoder = VulkanDecoder::new(self.clone())?;
        let frame_sorter = FrameSorter::<RawFrameData>::new();

        Ok(BytesDecoder {
            parser,
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
        let encoder = VulkanEncoder::new(self.clone(), parameters)?;
        Ok(BytesEncoder {
            vulkan_encoder: encoder,
        })
    }

    pub fn create_wgpu_textures_encoder(
        self: &Arc<Self>,
        parameters: EncoderParameters,
    ) -> Result<WgpuTexturesEncoder, VulkanEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(parameters)?;
        let encoder = VulkanEncoder::new_with_converter(self.clone(), parameters)?;
        Ok(WgpuTexturesEncoder {
            vulkan_encoder: encoder,
        })
    }

    pub fn encode_capabilities(&self) -> EncodeCapabilities {
        EncodeCapabilities {
            h264: Some(self.native_encode_capabilities.user_facing()),
        }
    }

    pub fn encoder_parameters_low_latency(
        &self,
        video_parameters: VideoParameters,
        rate_control: RateControl,
    ) -> EncoderParameters {
        EncoderParameters {
            video_parameters,
            profile: self.max_profile(),
            idr_period: None,
            max_references: None,
            rate_control,
            quality_level: 0,
        }
    }

    pub fn encoder_parameters_high_quality(
        &self,
        video_parameters: VideoParameters,
        rate_control: RateControl,
    ) -> EncoderParameters {
        EncoderParameters {
            video_parameters,
            profile: self.max_profile(),
            idr_period: None,
            max_references: None,
            rate_control,
            quality_level: self
                .native_encode_capabilities
                .profile(self.max_profile())
                .unwrap()
                .encode_capabilities
                .max_quality_levels
                - 1,
        }
    }

    fn max_profile(&self) -> H264Profile {
        if self.native_encode_capabilities.high.is_some() {
            H264Profile::High
        } else if self.native_encode_capabilities.main.is_some() {
            H264Profile::Main
        } else {
            H264Profile::Baseline
        }
    }

    fn validate_and_fill_encoder_parameters(
        &self,
        encoder_parameters: EncoderParameters,
    ) -> Result<FullEncoderParameters, VulkanEncoderError> {
        let native_profile_caps = self
            .native_encode_capabilities
            .profile(encoder_parameters.profile)
            .ok_or(VulkanEncoderError::ParametersError {
                field: "profile",
                problem: format!(
                    "Profile {:?} is not supported by this device.",
                    encoder_parameters.profile
                ),
            })?;

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

        Ok(FullEncoderParameters {
            idr_period,
            width,
            height,
            rate_control,
            max_references,
            quality_level: encoder_parameters.quality_level,
            profile: encoder_parameters.profile,
            framerate,
        })
    }
}

impl std::fmt::Debug for VulkanDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDevice").finish()
    }
}

pub(crate) struct ChosenDevice<'a> {
    pub(crate) physical_device: vk::PhysicalDevice,
    pub(crate) wgpu_adapter: wgpu::hal::ExposedAdapter<wgpu::hal::vulkan::Api>,
    pub(crate) queue_indices: QueueIndices<'a>,
    pub(crate) decode_capabilities: DecodeCapabilities,
    pub(crate) encode_capabilities: NativeEncodeCapabilities,
}

/// This macro will iterate over the `p_next` chain of the base struct until it finds a struct,
/// which matches the given type. After that it will execute the given action on the found struct.
///
/// # Example
/// ```ignore
/// unsafe {
///     find_ext!(queue_family_properties, found_extension @ ash::vk::QueueFamilyVideoPropertiesKHR => {
///         dbg!(found_extension)
///     });
/// }
/// ```
#[cfg_attr(doctest, macro_export)]
macro_rules! find_ext {
    ($base:expr, $var:ident @ $ext:ty => $action:stmt) => {
        let mut next = $base.p_next.cast::<ash::vk::BaseOutStructure>();
        while !next.is_null() {
            ash::match_out_struct!(match next {
                $var @ $ext => {
                    $action
                    break;
                }
            });

            next = (*next).p_next;
        }
    };
}

pub(crate) fn find_device<'a>(
    devices: &[vk::PhysicalDevice],
    instance: &Instance,
    wgpu_instance: &wgpu::Instance,
    required_extension_names: &[&CStr],
    compatible_surface: Option<&wgpu::Surface<'_>>,
) -> Result<ChosenDevice<'a>, VulkanInitError> {
    for &device in devices {
        let properties = unsafe { instance.get_physical_device_properties(device) };

        let wgpu_instance = unsafe { wgpu_instance.as_hal::<wgpu::hal::vulkan::Api>() }.unwrap();

        let wgpu_adapter = wgpu_instance
            .expose_adapter(device)
            .ok_or(VulkanInitError::WgpuAdapterNotCreated)?;

        if let Some(surface) = compatible_surface {
            let surface_capabilities = unsafe {
                (*surface)
                    .as_hal::<wgpu::hal::vulkan::Api>()
                    .and_then(|surface| wgpu_adapter.adapter.surface_capabilities(&surface))
            };

            if surface_capabilities.is_none() {
                continue;
            }
        }

        let mut vk_13_features = vk::PhysicalDeviceVulkan13Features::default();
        let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut vk_13_features);

        unsafe { instance.get_physical_device_features2(device, &mut features) };
        let extensions = unsafe { instance.enumerate_device_extension_properties(device)? };

        if vk_13_features.synchronization2 == 0 {
            warn!(
                "device {:?} does not support the required synchronization2 feature",
                properties.device_name_as_c_str()?
            );
        }

        if !required_extension_names.iter().all(|&extension_name| {
            extensions.iter().any(|ext| {
                let Ok(name) = ext.extension_name_as_c_str() else {
                    return false;
                };

                if name != extension_name {
                    return false;
                };

                true
            })
        }) {
            warn!(
                "device {:?} does not support the required extensions",
                properties.device_name_as_c_str()?
            );
            continue;
        }

        let queues_len =
            unsafe { instance.get_physical_device_queue_family_properties2_len(device) };
        let mut queues = vec![vk::QueueFamilyProperties2::default(); queues_len];
        let mut video_properties = vec![vk::QueueFamilyVideoPropertiesKHR::default(); queues_len];
        let mut query_result_status_properties =
            vec![vk::QueueFamilyQueryResultStatusPropertiesKHR::default(); queues_len];

        for ((queue, video_properties), query_result_properties) in queues
            .iter_mut()
            .zip(video_properties.iter_mut())
            .zip(query_result_status_properties.iter_mut())
        {
            *queue = queue
                .push_next(query_result_properties)
                .push_next(video_properties);
        }

        unsafe { instance.get_physical_device_queue_family_properties2(device, &mut queues) };

        let Some(decode_capabilities) = DecodeCapabilities::query(instance, device)? else {
            continue;
        };

        let encode_capabilities = NativeEncodeCapabilities::query(instance, device)?;

        let Some(transfer_queue_idx) = queues
            .iter()
            .enumerate()
            .find(|(_, q)| {
                q.queue_family_properties
                    .queue_flags
                    .contains(vk::QueueFlags::TRANSFER)
                    && !q
                        .queue_family_properties
                        .queue_flags
                        .intersects(vk::QueueFlags::GRAPHICS)
            })
            .map(|(i, _)| i)
        else {
            continue;
        };

        let Some(graphics_transfer_compute_queue_idx) = queues
            .iter()
            .enumerate()
            .find(|(_, q)| {
                q.queue_family_properties.queue_flags.contains(
                    vk::QueueFlags::GRAPHICS | vk::QueueFlags::TRANSFER | vk::QueueFlags::COMPUTE,
                )
            })
            .map(|(i, _)| i)
        else {
            continue;
        };

        let mut decode_queue_idx = None;
        for (i, queue) in queues.iter().enumerate() {
            if !queue
                .queue_family_properties
                .queue_flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR)
            {
                continue;
            }

            unsafe {
                find_ext!(queue, video_properties @ vk::QueueFamilyVideoPropertiesKHR =>
                    if video_properties
                        .video_codec_operations
                        .contains(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
                    {
                        decode_queue_idx = Some(i);
                    }
                );
            }
        }

        let Some(decode_queue_idx) = decode_queue_idx else {
            continue;
        };

        let mut encode_queue_idx = None;
        for (i, queue) in queues.iter().enumerate() {
            if !queue
                .queue_family_properties
                .queue_flags
                .contains(vk::QueueFlags::VIDEO_ENCODE_KHR)
            {
                continue;
            }

            unsafe {
                find_ext!(queue, video_properties @ vk::QueueFamilyVideoPropertiesKHR =>
                    if video_properties
                        .video_codec_operations
                        .contains(vk::VideoCodecOperationFlagsKHR::ENCODE_H264)
                    {
                        encode_queue_idx = Some(i);
                    }
                );
            }
        }

        let Some(encode_queue_idx) = encode_queue_idx else {
            continue;
        };

        debug!("decode capabilities: {decode_capabilities:#?}");
        debug!("encode capabilities: {encode_capabilities:#?}");

        return Ok(ChosenDevice {
            physical_device: device,
            wgpu_adapter,
            queue_indices: QueueIndices {
                transfer: QueueIndex {
                    idx: transfer_queue_idx,
                    video_properties: video_properties[transfer_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [transfer_queue_idx],
                },
                h264_decode: QueueIndex {
                    idx: decode_queue_idx,
                    video_properties: video_properties[decode_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [decode_queue_idx],
                },
                h264_encode: QueueIndex {
                    idx: encode_queue_idx,
                    video_properties: video_properties[encode_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [encode_queue_idx],
                },
                graphics_transfer_compute: QueueIndex {
                    idx: graphics_transfer_compute_queue_idx,
                    video_properties: video_properties[graphics_transfer_compute_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [graphics_transfer_compute_queue_idx],
                },
            },
            decode_capabilities,
            encode_capabilities,
        });
    }

    Err(VulkanInitError::NoDevice)
}
