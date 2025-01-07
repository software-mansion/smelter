use std::{
    ffi::{c_void, CStr},
    sync::Arc,
};

use ash::{vk, Entry};
use tracing::{debug, error, warn};
use wgpu::hal::Adapter;

use crate::{parser::Parser, vulkan_encoder::VulkanEncoder, wrappers::{Allocator, DebugMessenger, Device, Instance}, BytesDecoder, DecoderError, RateControl, RawFrameData, VulkanEncoderError, WgpuTexturesDecoder};

use super::{FrameSorter, VulkanDecoder};

const REQUIRED_EXTENSIONS: &[&CStr] = &[
    vk::KHR_VIDEO_QUEUE_NAME,
    vk::KHR_VIDEO_DECODE_QUEUE_NAME,
    vk::KHR_VIDEO_DECODE_H264_NAME,
    // TODO: We need a better mechanism for device selection, with feedback about which devices
    // support which operations. Some configurations might only have support for encode or decode,
    // and the user should be able to choose which one they want.
    vk::KHR_VIDEO_ENCODE_QUEUE_NAME,
    vk::KHR_VIDEO_ENCODE_H264_NAME,
];

#[derive(thiserror::Error, Debug)]
pub enum VulkanCtxError {
    #[error("Error loading vulkan: {0}")]
    LoadingError(#[from] ash::LoadingError),

    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("wgpu instance error: {0}")]
    WgpuInstanceError(#[from] wgpu::hal::InstanceError),

    #[error("wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),

    #[error("wgpu request device error: {0}")]
    WgpuRequestDeviceError(#[from] wgpu::RequestDeviceError),

    #[error("cannot create a wgpu adapter")]
    WgpuAdapterNotCreated,

    #[error("Cannot find a suitable physical device")]
    NoDevice,

    #[error("Cannot find a queue with index {0}")]
    NoQueue(usize),

    #[error("Memory copy requested to a bufer that is not set up for receiving input")]
    UploadToImproperBuffer,

    #[error("String conversion error: {0}")]
    StringConversionError(#[from] std::ffi::FromBytesUntilNulError),

    #[error("A slot in the Decoded Pictures Buffer was requested, but all slots are taken")]
    NoFreeSlotsInDpb,

    #[error("DPB can have at most 32 slots, {0} was requested")]
    DpbTooLong(u32),
}

/// Context for all encoders, decoders. Also contains a [`wgpu::Instance`].
pub struct VulkanInstance {
    wgpu_instance: wgpu::Instance,
    _entry: Arc<Entry>,
    instance: Arc<Instance>,
    _debug_messenger: Option<DebugMessenger>,
}

impl VulkanInstance {
    pub fn new() -> Result<Arc<Self>, VulkanCtxError> {
        let entry = Arc::new(unsafe { Entry::load()? });
        Self::new_from_entry(entry)
    }

    pub fn wgpu_instance(&self) -> wgpu::Instance {
        self.wgpu_instance.clone()
    }

    pub fn new_from(
        vulkan_library_path: impl AsRef<std::ffi::OsStr>,
    ) -> Result<Arc<Self>, VulkanCtxError> {
        let entry = Arc::new(unsafe { Entry::load_from(vulkan_library_path)? });
        Self::new_from_entry(entry)
    }

    fn new_from_entry(entry: Arc<Entry>) -> Result<Arc<Self>, VulkanCtxError> {
        let api_version = vk::make_api_version(0, 1, 3, 0);
        let app_info = vk::ApplicationInfo {
            api_version,
            ..Default::default()
        };

        let requested_layers = if cfg!(debug_assertions) {
            vec![c"VK_LAYER_KHRONOS_validation"]
        } else {
            Vec::new()
        };

        let instance_layer_properties = unsafe { entry.enumerate_instance_layer_properties()? };
        let instance_layer_names = instance_layer_properties
            .iter()
            .map(|layer| layer.layer_name_as_c_str())
            .collect::<Result<Vec<_>, _>>()?;

        let layers = requested_layers
            .into_iter()
            .filter(|requested_layer_name| {
                instance_layer_names
                    .iter()
                    .any(|instance_layer_name| instance_layer_name == requested_layer_name)
            })
            .map(|layer| layer.as_ptr())
            .collect::<Vec<_>>();

        let extensions = if cfg!(debug_assertions) {
            vec![vk::EXT_DEBUG_UTILS_NAME]
        } else {
            Vec::new()
        };

        let wgpu_extensions = wgpu::hal::vulkan::Instance::desired_extensions(
            &entry,
            api_version,
            wgpu::InstanceFlags::empty(),
        )?;

        let extensions = extensions
            .into_iter()
            .chain(wgpu_extensions)
            .collect::<Vec<_>>();

        let extension_ptrs = extensions.iter().map(|e| e.as_ptr()).collect::<Vec<_>>();

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extension_ptrs);

        let instance = unsafe { entry.create_instance(&create_info, None) }?;
        let video_queue_instance_ext = ash::khr::video_queue::Instance::new(&entry, &instance);
        let video_encode_queue_instance_ext =
            ash::khr::video_encode_queue::Instance::new(&entry, &instance);
        let debug_utils_instance_ext = ash::ext::debug_utils::Instance::new(&entry, &instance);

        let instance = Arc::new(Instance {
            instance,
            _entry: entry.clone(),
            video_queue_instance_ext,
            debug_utils_instance_ext,
            video_encode_queue_instance_ext,
        });

        let debug_messenger = if cfg!(debug_assertions) {
            Some(DebugMessenger::new(instance.clone())?)
        } else {
            None
        };

        let instance_clone = instance.clone();

        let wgpu_instance = unsafe {
            wgpu::hal::vulkan::Instance::from_raw(
                (*entry).clone(),
                instance.instance.clone(),
                api_version,
                0,
                None,
                extensions,
                wgpu::InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
                false,
                Some(Box::new(move || {
                    drop(instance_clone);
                })),
            )?
        };

        let wgpu_instance =
            unsafe { wgpu::Instance::from_hal::<wgpu::hal::vulkan::Api>(wgpu_instance) };

        Ok(Self {
            _entry: entry,
            instance,
            _debug_messenger: debug_messenger,
            wgpu_instance,
        }
        .into())
    }

    pub fn create_device(
        &self,
        wgpu_features: wgpu::Features,
        wgpu_limits: wgpu::Limits,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Arc<VulkanDevice>, VulkanCtxError> {
        let physical_devices = unsafe { self.instance.enumerate_physical_devices()? };

        let ChosenDevice {
            physical_device,
            wgpu_adapter,
            queue_indices,
            decode_capabilities,
            encode_capabilities,
        } = find_device(
            &physical_devices,
            &self.instance,
            &self.wgpu_instance,
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
            self.instance
                .create_device(physical_device, &device_create_info, None)?
        };
        let video_queue_ext = ash::khr::video_queue::Device::new(&self.instance, &device);
        let video_decode_queue_ext =
            ash::khr::video_decode_queue::Device::new(&self.instance, &device);

        let video_encode_queue_ext =
            ash::khr::video_encode_queue::Device::new(&self.instance, &device);

        let device = Arc::new(Device {
            device,
            video_queue_ext,
            video_decode_queue_ext,
            video_encode_queue_ext,
            _instance: self.instance.clone(),
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
            self.instance.clone(),
            physical_device,
            device.clone(),
        )?);

        let wgpu_adapter = unsafe { self.wgpu_instance.create_adapter_from_hal(wgpu_adapter) };
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
            encode_capabilities,
            wgpu_device: wgpu_device.into(),
            wgpu_queue: wgpu_queue.into(),
            wgpu_adapter: wgpu_adapter.into(),
        }
        .into())
    }
}

impl std::fmt::Debug for VulkanInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanInstance").finish()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum H264Profile {
    Baseline,
    Main,
    High,
}

impl H264Profile {
    pub(crate) fn to_profile_idc(self) -> vk::native::StdVideoH264ProfileIdc {
        match self {
            H264Profile::Baseline => {
                vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE
            }
            H264Profile::Main => vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN,
            H264Profile::High => vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EncodeCapabilities {
    pub(crate) baseline: Option<EncodeProfileCapabilities>,
    pub(crate) main: Option<EncodeProfileCapabilities>,
    pub(crate) high: Option<EncodeProfileCapabilities>,
}

impl EncodeCapabilities {
    fn query(instance: &Instance, device: vk::PhysicalDevice) -> Result<Self, VulkanCtxError> {
        let baseline = EncodeProfileCapabilities::query(
            instance,
            device,
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE,
        )
        .ok();
        let main = EncodeProfileCapabilities::query(
            instance,
            device,
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN,
        )
        .ok();
        let high = EncodeProfileCapabilities::query(
            instance,
            device,
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH,
        )
        .ok();

        Ok(Self {
            baseline,
            main,
            high,
        })
    }

    pub(crate) fn profile(&self, profile: H264Profile) -> Option<&EncodeProfileCapabilities> {
        match profile {
            H264Profile::Baseline => self.baseline.as_ref(),
            H264Profile::Main => self.main.as_ref(),
            H264Profile::High => self.high.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EncodeProfileCapabilities {
    pub(crate) video_capabilities: vk::VideoCapabilitiesKHR<'static>,
    pub(crate) encode_capabilities: vk::VideoEncodeCapabilitiesKHR<'static>,
    pub(crate) h264_encode_capabilities: vk::VideoEncodeH264CapabilitiesKHR<'static>,
    pub(crate) encode_dpb_properties: Vec<vk::VideoFormatPropertiesKHR<'static>>,
    pub(crate) encode_src_properties: Vec<vk::VideoFormatPropertiesKHR<'static>>,
    pub(crate) quality_level_properties: Vec<EncodeQualityLevelProperties>,
}

impl EncodeProfileCapabilities {
    fn query(
        instance: &Instance,
        device: vk::PhysicalDevice,
        profile: vk::native::StdVideoH264ProfileIdc,
    ) -> Result<Self, VulkanCtxError> {
        let mut h264_encode_profile_info =
            vk::VideoEncodeH264ProfileInfoKHR::default().std_profile_idc(profile);

        let encode_profile_info = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::ENCODE_H264)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .push_next(&mut h264_encode_profile_info);

        let encode_dpb_properties = query_video_format_properties(
            device,
            &instance.video_queue_instance_ext,
            &encode_profile_info,
            vk::ImageUsageFlags::VIDEO_ENCODE_DPB_KHR,
        )?;

        let encode_src_properties = query_video_format_properties(
            device,
            &instance.video_queue_instance_ext,
            &encode_profile_info,
            vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR,
        )?;

        let mut h264_encode_caps = vk::VideoEncodeH264CapabilitiesKHR::default();
        let mut encode_caps = vk::VideoEncodeCapabilitiesKHR {
            p_next: (&mut h264_encode_caps as *mut _) as *mut c_void,
            ..Default::default()
        };
        let mut caps = vk::VideoCapabilitiesKHR::default().push_next(&mut encode_caps);

        unsafe {
            (instance
                .video_queue_instance_ext
                .fp()
                .get_physical_device_video_capabilities_khr)(
                device,
                &encode_profile_info,
                &mut caps,
            )
            .result()?;
        }

        let video_capabilities = vk::VideoCapabilitiesKHR::default()
            .flags(caps.flags)
            .min_bitstream_buffer_offset_alignment(caps.min_bitstream_buffer_offset_alignment)
            .min_bitstream_buffer_size_alignment(caps.min_bitstream_buffer_size_alignment)
            .picture_access_granularity(caps.picture_access_granularity)
            .min_coded_extent(caps.min_coded_extent)
            .max_coded_extent(caps.max_coded_extent)
            .max_dpb_slots(caps.max_dpb_slots)
            .max_active_reference_pictures(caps.max_active_reference_pictures)
            .std_header_version(caps.std_header_version);

        let encode_capabilities = vk::VideoEncodeCapabilitiesKHR::default()
            .flags(encode_caps.flags)
            .rate_control_modes(encode_caps.rate_control_modes)
            .max_rate_control_layers(encode_caps.max_rate_control_layers)
            .max_bitrate(encode_caps.max_bitrate)
            .max_quality_levels(encode_caps.max_quality_levels)
            .encode_input_picture_granularity(encode_caps.encode_input_picture_granularity)
            .supported_encode_feedback_flags(encode_caps.supported_encode_feedback_flags);

        let h264_encode_capabilities = vk::VideoEncodeH264CapabilitiesKHR::default()
            .flags(h264_encode_caps.flags)
            .max_level_idc(h264_encode_caps.max_level_idc)
            .max_slice_count(h264_encode_caps.max_slice_count)
            .max_p_picture_l0_reference_count(h264_encode_caps.max_p_picture_l0_reference_count)
            .max_b_picture_l0_reference_count(h264_encode_caps.max_b_picture_l0_reference_count)
            .max_l1_reference_count(h264_encode_caps.max_l1_reference_count)
            .max_temporal_layer_count(h264_encode_caps.max_temporal_layer_count)
            .expect_dyadic_temporal_layer_pattern(
                h264_encode_caps.expect_dyadic_temporal_layer_pattern != 0,
            )
            .min_qp(h264_encode_caps.min_qp)
            .max_qp(h264_encode_caps.max_qp)
            .prefers_gop_remaining_frames(h264_encode_caps.prefers_gop_remaining_frames != 0)
            .requires_gop_remaining_frames(h264_encode_caps.requires_gop_remaining_frames != 0)
            .std_syntax_flags(h264_encode_caps.std_syntax_flags);

        let mut quality_level_properties =
            Vec::with_capacity(encode_capabilities.max_quality_levels as usize);

        for i in 0..encode_capabilities.max_quality_levels {
            if let Ok(qlp) =
                EncodeQualityLevelProperties::query(instance, device, &encode_profile_info, i)
            {
                quality_level_properties.push(qlp);
            }
        }

        Ok(Self {
            video_capabilities,
            encode_capabilities,
            h264_encode_capabilities,
            encode_dpb_properties,
            encode_src_properties,
            quality_level_properties,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EncodeQualityLevelProperties {
    pub(crate) quality_level_properties: vk::VideoEncodeQualityLevelPropertiesKHR<'static>,
    pub(crate) h264_quality_level_properties: vk::VideoEncodeH264QualityLevelPropertiesKHR<'static>,
}

impl EncodeQualityLevelProperties {
    fn query(
        instance: &Instance,
        device: vk::PhysicalDevice,
        profile_info: &vk::VideoProfileInfoKHR<'_>,
        quality_level: u32,
    ) -> Result<Self, VulkanCtxError> {
        let quality_level_info = vk::PhysicalDeviceVideoEncodeQualityLevelInfoKHR::default()
            .video_profile(profile_info)
            .quality_level(quality_level);

        let mut h264_qlp = vk::VideoEncodeH264QualityLevelPropertiesKHR::default();
        let mut qlp = vk::VideoEncodeQualityLevelPropertiesKHR::default().push_next(&mut h264_qlp);

        unsafe {
            (instance
                .video_encode_queue_instance_ext
                .fp()
                .get_physical_device_video_encode_quality_level_properties_khr)(
                device,
                &quality_level_info,
                &mut qlp,
            )
            .result()?;
        }

        let quality_level_properties = vk::VideoEncodeQualityLevelPropertiesKHR::default()
            .preferred_rate_control_mode(qlp.preferred_rate_control_mode)
            .preferred_rate_control_layer_count(qlp.preferred_rate_control_layer_count);

        let h264_quality_level_properties = vk::VideoEncodeH264QualityLevelPropertiesKHR::default()
            .preferred_rate_control_flags(h264_qlp.preferred_rate_control_flags)
            .preferred_gop_frame_count(h264_qlp.preferred_gop_frame_count)
            .preferred_idr_period(h264_qlp.preferred_idr_period)
            .preferred_consecutive_b_frame_count(h264_qlp.preferred_consecutive_b_frame_count)
            .preferred_temporal_layer_count(h264_qlp.preferred_temporal_layer_count)
            .preferred_constant_qp(h264_qlp.preferred_constant_qp)
            .preferred_max_l0_reference_count(h264_qlp.preferred_max_l0_reference_count)
            .preferred_max_l1_reference_count(h264_qlp.preferred_max_l1_reference_count)
            .preferred_std_entropy_coding_mode_flag(
                h264_qlp.preferred_std_entropy_coding_mode_flag != 0,
            );

        Ok(Self {
            quality_level_properties,
            h264_quality_level_properties,
        })
    }

    pub(crate) fn zeroed(&self) -> bool {
        // this is hideous
        self.quality_level_properties
            .preferred_rate_control_mode
            .as_raw()
            == 0
            && self
                .quality_level_properties
                .preferred_rate_control_layer_count
                == 0
            && self
                .h264_quality_level_properties
                .preferred_rate_control_flags
                .as_raw()
                == 0
            && self.h264_quality_level_properties.preferred_gop_frame_count == 0
            && self.h264_quality_level_properties.preferred_idr_period == 0
            && self
                .h264_quality_level_properties
                .preferred_consecutive_b_frame_count
                == 0
            && self
                .h264_quality_level_properties
                .preferred_temporal_layer_count
                == 0
            && self
                .h264_quality_level_properties
                .preferred_constant_qp
                .qp_i
                == 0
            && self
                .h264_quality_level_properties
                .preferred_constant_qp
                .qp_p
                == 0
            && self
                .h264_quality_level_properties
                .preferred_constant_qp
                .qp_b
                == 0
            && self
                .h264_quality_level_properties
                .preferred_max_l0_reference_count
                == 0
            && self
                .h264_quality_level_properties
                .preferred_max_l1_reference_count
                == 0
            && self
                .h264_quality_level_properties
                .preferred_std_entropy_coding_mode_flag
                == 0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DecodeCapabilities {
    pub(crate) video_capabilities: vk::VideoCapabilitiesKHR<'static>,
    pub(crate) decode_capabilities: vk::VideoDecodeCapabilitiesKHR<'static>,
    pub(crate) h264_decode_capabilities: vk::VideoDecodeH264CapabilitiesKHR<'static>,
    pub(crate) h264_dpb_format_properties: vk::VideoFormatPropertiesKHR<'static>,
    pub(crate) h264_dst_format_properties: Option<vk::VideoFormatPropertiesKHR<'static>>,
}

impl DecodeCapabilities {
    fn query(
        instance: &Instance,
        device: vk::PhysicalDevice,
    ) -> Result<Option<Self>, VulkanCtxError> {
        let mut h264_decode_profile_info = vk::VideoDecodeH264ProfileInfoKHR::default()
            .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE)
            .std_profile_idc(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH);

        let decode_profile_info = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .push_next(&mut h264_decode_profile_info);

        let mut h264_decode_caps = vk::VideoDecodeH264CapabilitiesKHR::default();
        let mut decode_caps = vk::VideoDecodeCapabilitiesKHR {
            p_next: (&mut h264_decode_caps as *mut _) as *mut c_void, // why does this not have `.push_next()`? wtf
            ..Default::default()
        };

        let mut caps = vk::VideoCapabilitiesKHR::default().push_next(&mut decode_caps);

        unsafe {
            (instance
                .video_queue_instance_ext
                .fp()
                .get_physical_device_video_capabilities_khr)(
                device,
                &decode_profile_info,
                &mut caps,
            )
            .result()?
        };

        let video_capabilities = vk::VideoCapabilitiesKHR::default()
            .flags(caps.flags)
            .min_bitstream_buffer_size_alignment(caps.min_bitstream_buffer_size_alignment)
            .min_bitstream_buffer_offset_alignment(caps.min_bitstream_buffer_offset_alignment)
            .picture_access_granularity(caps.picture_access_granularity)
            .min_coded_extent(caps.min_coded_extent)
            .max_coded_extent(caps.max_coded_extent)
            .max_dpb_slots(caps.max_dpb_slots)
            .max_active_reference_pictures(caps.max_active_reference_pictures)
            .std_header_version(caps.std_header_version);

        let decode_capabilities =
            vk::VideoDecodeCapabilitiesKHR::default().flags(decode_caps.flags);

        let h264_decode_capabilities = vk::VideoDecodeH264CapabilitiesKHR::default()
            .max_level_idc(h264_decode_caps.max_level_idc)
            .field_offset_granularity(h264_decode_caps.field_offset_granularity);

        let flags = decode_caps.flags;

        let h264_dpb_format_properties =
            if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE) {
                query_video_format_properties(
                    device,
                    &instance.video_queue_instance_ext,
                    &decode_profile_info,
                    vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                        | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                )?
            } else {
                query_video_format_properties(
                    device,
                    &instance.video_queue_instance_ext,
                    &decode_profile_info,
                    vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR,
                )?
            };

        let h264_dst_format_properties =
            if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE) {
                None
            } else {
                Some(query_video_format_properties(
                    device,
                    &instance.video_queue_instance_ext,
                    &decode_profile_info,
                    vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::TRANSFER_SRC,
                )?)
            };

        let h264_dpb_format_properties = match h264_dpb_format_properties
            .into_iter()
            .find(|f| f.format == vk::Format::G8_B8R8_2PLANE_420_UNORM)
        {
            Some(f) => f,
            None => return Ok(None),
        };

        let h264_dst_format_properties = match h264_dst_format_properties {
            Some(format_properties) => match format_properties
                .into_iter()
                .find(|f| f.format == vk::Format::G8_B8R8_2PLANE_420_UNORM)
            {
                Some(f) => Some(f),
                None => return Ok(None),
            },
            None => None,
        };

        Ok(Some(Self {
            video_capabilities,
            decode_capabilities,
            h264_decode_capabilities,
            h264_dpb_format_properties,
            h264_dst_format_properties,
        }))
    }
}

/// Open connection to a coding-capable device. Also contains a [`wgpu::Device`], a [`wgpu::Queue`] and
/// a [`wgpu::Adapter`].
pub struct VulkanDevice {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) wgpu_adapter: wgpu::Adapter,
    _physical_device: vk::PhysicalDevice,
    pub(crate) device: Arc<Device>,
    pub(crate) allocator: Arc<Allocator>,
    pub(crate) queues: Queues,
    pub(crate) decode_capabilities: DecodeCapabilities,
    pub(crate) encode_capabilities: EncodeCapabilities,
}

impl VulkanDevice {
    pub fn create_wgpu_textures_decoder(
        self: &Arc<Self>,
    ) -> Result<WgpuTexturesDecoder<'static>, DecoderError> {
        let parser = Parser::default();
        let vulkan_decoder = VulkanDecoder::new(self.clone())?;
        let frame_sorter = FrameSorter::<wgpu::Texture>::new();

        Ok(WgpuTexturesDecoder {
            parser,
            vulkan_decoder,
            frame_sorter,
        })
    }

    pub fn create_bytes_decoder(self: &Arc<Self>) -> Result<BytesDecoder<'_>, DecoderError> {
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

    pub fn crate_encoder(
        self: &Arc<Self>,
        profile: H264Profile,
        width: u32,
        height: u32,
        gop_size: usize,
        rate_control: RateControl,
    ) -> Result<VulkanEncoder, VulkanEncoderError> {
        VulkanEncoder::new(self.clone(), profile, width, height, gop_size, rate_control)
    }

    pub(crate) fn queue_from_index(&self, idx: usize) -> Result<&Queue, VulkanCtxError> {
        self.iter_queues()
            .find(|q| q.idx == idx)
            .ok_or(VulkanCtxError::NoQueue(idx))
    }

    fn iter_queues(&self) -> impl Iterator<Item = &Queue> {
        [
            &self.queues.wgpu,
            &self.queues.h264_decode,
            &self.queues.h264_encode,
            &self.queues.transfer,
        ]
        .into_iter()
    }
}

impl std::fmt::Debug for VulkanDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDevice").finish()
    }
}

struct ChosenDevice<'a> {
    physical_device: vk::PhysicalDevice,
    wgpu_adapter: wgpu::hal::ExposedAdapter<wgpu::hal::vulkan::Api>,
    queue_indices: QueueIndices<'a>,
    decode_capabilities: DecodeCapabilities,
    encode_capabilities: EncodeCapabilities,
}

/// This macro will iterate over the `p_next` chain of the base struct until it finds a struct,
/// which matches the given type. After that it will execute the given action on the found struct.
///
/// # Example
/// ```rust
/// unsafe {
///     find_ext!(queue_family_properties, found_extension @ ash::vk::QueueFamilyVideoPropertiesKHR => {
///         dbg!(found_extension)
///     });
/// }
/// ```
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

fn find_device<'a>(
    devices: &[vk::PhysicalDevice],
    instance: &Instance,
    wgpu_instance: &wgpu::Instance,
    required_extension_names: &[&CStr],
    compatible_surface: Option<&wgpu::Surface<'_>>,
) -> Result<ChosenDevice<'a>, VulkanCtxError> {
    for &device in devices {
        let properties = unsafe { instance.get_physical_device_properties(device) };

        let wgpu_instance = unsafe { wgpu_instance.as_hal::<wgpu::hal::vulkan::Api>() }.unwrap();

        let wgpu_adapter = wgpu_instance
            .expose_adapter(device)
            .ok_or(VulkanCtxError::WgpuAdapterNotCreated)?;

        if let Some(surface) = compatible_surface {
            let surface_capabilities = unsafe {
                (*surface).as_hal::<wgpu::hal::vulkan::Api, _, _>(|surface| {
                    surface.and_then(|surface| wgpu_adapter.adapter.surface_capabilities(surface))
                })
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

        let encode_capabilities = EncodeCapabilities::query(instance, device)?;

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

    Err(VulkanCtxError::NoDevice)
}

fn query_video_format_properties<'a>(
    device: vk::PhysicalDevice,
    video_queue_instance_ext: &ash::khr::video_queue::Instance,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    image_usage: vk::ImageUsageFlags,
) -> Result<Vec<vk::VideoFormatPropertiesKHR<'a>>, VulkanCtxError> {
    let mut profile_list_info =
        vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));

    let format_info = vk::PhysicalDeviceVideoFormatInfoKHR::default()
        .image_usage(image_usage)
        .push_next(&mut profile_list_info);

    let mut format_info_length = 0;

    unsafe {
        (video_queue_instance_ext
            .fp()
            .get_physical_device_video_format_properties_khr)(
            device,
            &format_info,
            &mut format_info_length,
            std::ptr::null_mut(),
        )
        .result()?;
    }

    let mut format_properties =
        vec![vk::VideoFormatPropertiesKHR::default(); format_info_length as usize];

    unsafe {
        (video_queue_instance_ext
            .fp()
            .get_physical_device_video_format_properties_khr)(
            device,
            &format_info,
            &mut format_info_length,
            format_properties.as_mut_ptr(),
        )
        .result()?;
    }

    Ok(format_properties)
}

pub(crate) struct Queue {
    pub(crate) queue: std::sync::Mutex<vk::Queue>,
    pub(crate) idx: usize,
    _video_properties: vk::QueueFamilyVideoPropertiesKHR<'static>,
    pub(crate) query_result_status_properties:
        vk::QueueFamilyQueryResultStatusPropertiesKHR<'static>,
    device: Arc<Device>,
}

impl Queue {
    pub(crate) fn supports_result_status_queries(&self) -> bool {
        self.query_result_status_properties
            .query_result_status_support
            == vk::TRUE
    }

    pub(crate) fn submit(
        &self,
        buffer: &CommandBuffer,
        wait_semaphores: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        signal_semaphores: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        fence: Option<vk::Fence>,
    ) -> Result<(), VulkanCtxError> {
        fn to_sem_submit_info(
            submits: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        ) -> Vec<vk::SemaphoreSubmitInfo<'_>> {
            submits
                .iter()
                .map(|&(sem, stage)| {
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(sem)
                        .stage_mask(stage)
                })
                .collect::<Vec<_>>()
        }

        let wait_semaphores = to_sem_submit_info(wait_semaphores);
        let signal_semaphores = to_sem_submit_info(signal_semaphores);

        let buffer_submit_info =
            [vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer)];

        let submit_info = [vk::SubmitInfo2::default()
            .wait_semaphore_infos(&wait_semaphores)
            .signal_semaphore_infos(&signal_semaphores)
            .command_buffer_infos(&buffer_submit_info)];

        unsafe {
            self.device.queue_submit2(
                *self.queue.lock().unwrap(),
                &submit_info,
                fence.unwrap_or(vk::Fence::null()),
            )?
        };

        Ok(())
    }
}

pub(crate) struct Queues {
    pub(crate) transfer: Queue,
    pub(crate) h264_decode: Queue,
    pub(crate) h264_encode: Queue,
    pub(crate) wgpu: Queue,
}

struct QueueIndex<'a> {
    idx: usize,
    video_properties: vk::QueueFamilyVideoPropertiesKHR<'a>,
    query_result_status_properties: vk::QueueFamilyQueryResultStatusPropertiesKHR<'a>,
}

pub(crate) struct QueueIndices<'a> {
    transfer: QueueIndex<'a>,
    h264_decode: QueueIndex<'a>,
    h264_encode: QueueIndex<'a>,
    graphics_transfer_compute: QueueIndex<'a>,
}

impl QueueIndices<'_> {
    fn queue_create_infos(&self) -> Vec<vk::DeviceQueueCreateInfo<'_>> {
        [
            self.h264_decode.idx,
            self.h264_encode.idx,
            self.transfer.idx,
            self.graphics_transfer_compute.idx,
        ]
        .into_iter()
        .collect::<std::collections::HashSet<usize>>()
        .into_iter()
        .map(|i| {
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(i as u32)
                .queue_priorities(&[1.0])
        })
        .collect::<Vec<_>>()
    }
}
