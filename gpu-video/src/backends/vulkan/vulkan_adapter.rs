use ash::vk;
use std::ffi::CStr;
use tracing::{debug, debug_span, warn};

use crate::{
    VideoDeviceInitError,
    adapter::{DeviceType, VideoAdapterBackend, VideoAdapterInfo},
    backends::vulkan::{
        VulkanDevice,
        vulkan_device::{
            caps::{NativeDecodeCapabilities, NativeEncodeCapabilities},
            queues::{QueueIndex, QueueIndices},
        },
        vulkan_instance::VulkanInstance,
    },
    capabilities::{DecodeCapabilities, EncodeCapabilities},
    device::VideoDeviceDescriptor,
};

const REQUIRED_EXTENSIONS: &[&CStr] = &[vk::KHR_VIDEO_QUEUE_NAME, vk::KHR_VIDEO_MAINTENANCE1_NAME];

const DECODE_EXTENSIONS: &[&CStr] = &[vk::KHR_VIDEO_DECODE_QUEUE_NAME];
const DECODE_CODEC_EXTENSIONS: &[&CStr] = &[
    vk::KHR_VIDEO_DECODE_H264_NAME,
    vk::KHR_VIDEO_DECODE_H265_NAME,
];

const ENCODE_EXTENSIONS: &[&CStr] = &[vk::KHR_VIDEO_ENCODE_QUEUE_NAME];
const ENCODE_CODEC_EXTENSIONS: &[&CStr] = &[
    vk::KHR_VIDEO_ENCODE_H264_NAME,
    vk::KHR_VIDEO_ENCODE_H265_NAME,
];

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub(crate) use wgpu_api::*;

/// Represents a handle to a physical device.
/// Can be used to create [`VideoDevice`](crate::VideoDevice).
pub struct VulkanAdapter<'a> {
    pub(crate) instance: &'a VulkanInstance,
    pub(crate) physical_device: vk::PhysicalDevice,
    pub(crate) queue_indices: QueueIndices<'static>,
    pub(crate) decode_capabilities: Option<NativeDecodeCapabilities>,
    pub(crate) encode_capabilities: Option<NativeEncodeCapabilities>,
    pub(crate) info: VulkanAdapterInfo,
}

impl<'a> VulkanAdapter<'a> {
    pub(crate) fn new(
        vulkan_instance: &'a VulkanInstance,
        device: vk::PhysicalDevice,
    ) -> Option<Self> {
        let instance = &vulkan_instance.instance;
        let properties = unsafe { instance.get_physical_device_properties(device) };
        let device_name = properties
            .device_name_as_c_str()
            .map(CStr::to_string_lossy)
            .unwrap_or("unknown".into());

        let _span = debug_span!("creating adapter", device_name = %device_name).entered();

        let mut vk_13_features = vk::PhysicalDeviceVulkan13Features::default();
        let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut vk_13_features);

        unsafe { instance.get_physical_device_features2(device, &mut features) };
        let extensions = match unsafe { instance.enumerate_device_extension_properties(device) } {
            Ok(ext) => ext,
            Err(err) => {
                warn!("Couldn't enumerate device extension properties: {err}");
                return None;
            }
        };

        if vk_13_features.synchronization2 == vk::FALSE {
            debug!("device does not support the required synchronization2 feature");
            return None;
        }

        if let Err(missing) = check_extensions(REQUIRED_EXTENSIONS, &extensions) {
            debug!(missing_extensions = ?missing, "device is missing some required extensions",);
            return None;
        }

        let has_decode_extensions = check_extensions(DECODE_EXTENSIONS, &extensions).is_ok();
        let supported_decode_codec_extensions =
            supported_extensions(DECODE_CODEC_EXTENSIONS, &extensions);
        let supports_any_decoding =
            has_decode_extensions && !supported_decode_codec_extensions.is_empty();
        let supported_decode_operations =
            extensions_to_codec_operations(&supported_decode_codec_extensions);

        let has_encode_extensions = check_extensions(ENCODE_EXTENSIONS, &extensions).is_ok();
        let supported_encode_codec_extensions =
            supported_extensions(ENCODE_CODEC_EXTENSIONS, &extensions);
        let supports_any_encoding =
            has_encode_extensions && !supported_encode_codec_extensions.is_empty();
        let supported_encode_operations =
            extensions_to_codec_operations(&supported_encode_codec_extensions);

        if !supports_any_decoding && !supports_any_encoding {
            debug!("device does not support encoding or decoding extensions");
            return None;
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

        let decode_capabilities =
            NativeDecodeCapabilities::query(instance, device, supported_decode_operations);
        let encode_capabilities =
            NativeEncodeCapabilities::query(instance, device, supported_encode_operations);

        let queue_counts = queues
            .iter()
            .map(|q| q.queue_family_properties.queue_count)
            .collect::<Vec<_>>();

        let transfer_queue_idx = queues
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
            .map(|(i, _)| i)?;

        let graphics_transfer_compute_queue_idx = queues
            .iter()
            .enumerate()
            .find(|(_, q)| {
                q.queue_family_properties.queue_flags.contains(
                    vk::QueueFlags::GRAPHICS | vk::QueueFlags::TRANSFER | vk::QueueFlags::COMPUTE,
                )
            })
            .map(|(i, _)| i)?;

        let compute_queue_idx = queues
            .iter()
            .enumerate()
            .find(|(_, q)| {
                q.queue_family_properties
                    .queue_flags
                    .contains(vk::QueueFlags::COMPUTE)
                    && !q
                        .queue_family_properties
                        .queue_flags
                        .intersects(vk::QueueFlags::GRAPHICS)
            })
            // Fall back to any compute-capable queue when no dedicated one exists
            // (e.g. NVIDIA Maxwell only exposes COMPUTE on the graphics family).
            // If the candidate is wgpu's family it must have a second queue for
            // us, since two threads must not submit to the same VkQueue.
            .or_else(|| {
                queues.iter().enumerate().find(|(i, q)| {
                    q.queue_family_properties
                        .queue_flags
                        .contains(vk::QueueFlags::COMPUTE)
                        && (*i != graphics_transfer_compute_queue_idx
                            || q.queue_family_properties.queue_count >= 2)
                })
            })
            .map(|(i, _)| i)?;

        let decode_queue_idx = match supports_any_decoding {
            true => find_video_queue_idx(
                &queues,
                vk::QueueFlags::VIDEO_DECODE_KHR,
                // TODO: for now, we only look for a single queue that supports all decoding
                supported_decode_operations,
            ),
            false => None,
        };
        let encode_queue_idx = match supports_any_encoding {
            true => find_video_queue_idx(
                &queues,
                vk::QueueFlags::VIDEO_ENCODE_KHR,
                // TODO: for now, we only look for a single queue that supports all encoding
                supported_encode_operations,
            ),
            false => None,
        };

        if decode_queue_idx.is_none() && encode_queue_idx.is_none() {
            debug!("device does not have any queues that support video operations");
            return None;
        }

        debug!("decode capabilities: {decode_capabilities:#?}");
        debug!("encode capabilities: {encode_capabilities:#?}");

        let (driver_name, driver_info) = match properties.api_version >= vk::API_VERSION_1_2 {
            true => {
                let mut driver_properties = vk::PhysicalDeviceDriverProperties::default();
                let mut properties2 =
                    vk::PhysicalDeviceProperties2::default().push_next(&mut driver_properties);
                unsafe {
                    instance.get_physical_device_properties2(device, &mut properties2);
                }

                let driver_name = driver_properties
                    .driver_name_as_c_str()
                    .map(CStr::to_string_lossy)
                    .unwrap_or("unknown".into())
                    .into_owned();
                let driver_info = driver_properties
                    .driver_info_as_c_str()
                    .map(CStr::to_string_lossy)
                    .unwrap_or_default()
                    .into_owned();
                (driver_name, driver_info)
            }
            false => ("unknown".to_owned(), "".to_owned()),
        };

        let info = VulkanAdapterInfo {
            name: device_name.into_owned(),
            driver_name,
            driver_info,
            device_type: properties.device_type,
            device_properties: properties,
            supports_decoding: decode_queue_idx.is_some(),
            supports_encoding: encode_queue_idx.is_some(),
            decode_capabilities: decode_capabilities.user_facing(),
            encode_capabilities: encode_capabilities.user_facing(),
        };

        Some(Self {
            instance: vulkan_instance,
            physical_device: device,
            queue_indices: QueueIndices {
                transfer: QueueIndex {
                    family_index: transfer_queue_idx,
                    queue_count: queue_counts[transfer_queue_idx] as usize,
                    video_properties: video_properties[transfer_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [transfer_queue_idx],
                },
                compute: QueueIndex {
                    family_index: compute_queue_idx,
                    // On fallback to wgpu's family, take two queues: index 0 for wgpu,
                    // index 1 for gpu-video (the fallback guarantees two exist).
                    queue_count: if compute_queue_idx == graphics_transfer_compute_queue_idx {
                        2
                    } else {
                        queue_counts[compute_queue_idx] as usize
                    },
                    video_properties: video_properties[compute_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [compute_queue_idx],
                },
                h264_decode: decode_queue_idx.map(|idx| QueueIndex {
                    family_index: idx,
                    queue_count: queue_counts[idx] as usize,
                    video_properties: video_properties[idx],
                    query_result_status_properties: query_result_status_properties[idx],
                }),
                encode: encode_queue_idx.map(|idx| QueueIndex {
                    family_index: idx,
                    queue_count: queue_counts[idx] as usize,
                    video_properties: video_properties[idx],
                    query_result_status_properties: query_result_status_properties[idx],
                }),
                graphics_transfer_compute: QueueIndex {
                    family_index: graphics_transfer_compute_queue_idx,
                    queue_count: 1, // Currently we can only handle 1 queue
                    video_properties: video_properties[graphics_transfer_compute_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [graphics_transfer_compute_queue_idx],
                },
            },
            decode_capabilities: if supports_any_decoding {
                Some(decode_capabilities)
            } else {
                None
            },
            encode_capabilities: if supports_any_encoding {
                Some(encode_capabilities)
            } else {
                None
            },
            info,
        })
    }

    pub fn supports_decoding(&self) -> bool {
        self.info.supports_decoding
    }

    pub fn supports_encoding(&self) -> bool {
        self.info.supports_encoding
    }

    pub fn info(&self) -> &VulkanAdapterInfo {
        &self.info
    }

    pub(crate) fn required_extensions(&self) -> Vec<&'static CStr> {
        REQUIRED_EXTENSIONS
            .iter()
            .copied()
            .chain(match self.supports_decoding() {
                true => DECODE_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .chain(match self.supports_decoding() {
                true => DECODE_CODEC_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .chain(match self.supports_encoding() {
                true => ENCODE_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .chain(match self.supports_encoding() {
                true => ENCODE_CODEC_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .collect::<Vec<_>>()
    }

    pub fn create_device(
        self,
        desc: &VideoDeviceDescriptor,
    ) -> Result<crate::VideoDevice, VideoDeviceInitError> {
        VulkanDevice::create(self, desc.clone()).map_err(Into::into)
    }
}

impl VideoAdapterBackend for VulkanAdapter<'_> {
    fn build_info(&self) -> VideoAdapterInfo {
        let VulkanAdapterInfo {
            name,
            driver_name,
            driver_info,
            device_type,
            supports_decoding,
            supports_encoding,
            device_properties,
            decode_capabilities,
            encode_capabilities,
        } = self.info().clone();

        let api_version = {
            let version = self.info.device_properties.api_version;
            let major = vk::api_version_major(version);
            let minor = vk::api_version_minor(version);
            let patch = vk::api_version_patch(version);

            format!("{major}.{minor}.{patch}")
        };

        let device_type = match device_type {
            vk::PhysicalDeviceType::OTHER => DeviceType::Other,
            vk::PhysicalDeviceType::INTEGRATED_GPU => DeviceType::IntegratedGpu,
            vk::PhysicalDeviceType::DISCRETE_GPU => DeviceType::DiscreteGpu,
            vk::PhysicalDeviceType::VIRTUAL_GPU => DeviceType::VirtualGpu,
            vk::PhysicalDeviceType::CPU => DeviceType::Cpu,
            _ => DeviceType::Other,
        };

        VideoAdapterInfo {
            name,
            driver_name,
            driver_info,
            device: device_properties.device_id.to_string(),
            vendor: device_properties.vendor_id.to_string(),
            device_type,
            api_version,
            supports_decoding,
            supports_encoding,
            decode_capabilities,
            encode_capabilities,
        }
    }

    fn create_device(
        self: Box<Self>,
        desc: &VideoDeviceDescriptor,
    ) -> Result<crate::VideoDevice, VideoDeviceInitError> {
        VulkanAdapter::create_device(*self, desc)
    }
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

/// Returns the list of missing extensions
fn check_extensions<'a>(
    required_extensions: &'a [&'a CStr],
    available_extensions: &'a [vk::ExtensionProperties],
) -> Result<(), Vec<&'a CStr>> {
    let missing = required_extensions
        .iter()
        .copied()
        .filter(|&required_name| {
            !available_extensions.iter().any(|ext| {
                let Ok(name) = ext.extension_name_as_c_str() else {
                    return false;
                };

                name == required_name
            })
        })
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        return Err(missing);
    }

    Ok(())
}

fn supported_extensions<'a>(
    required_extensions: &'a [&'a CStr],
    available_extensions: &'a [vk::ExtensionProperties],
) -> Vec<&'a CStr> {
    required_extensions
        .iter()
        .copied()
        .filter(|&required_name| {
            available_extensions.iter().any(|ext| {
                let Ok(name) = ext.extension_name_as_c_str() else {
                    return false;
                };

                name == required_name
            })
        })
        .collect()
}

fn extensions_to_codec_operations(extensions: &[&CStr]) -> vk::VideoCodecOperationFlagsKHR {
    extensions
        .iter()
        .copied()
        .fold(vk::VideoCodecOperationFlagsKHR::empty(), |acc, ext| {
            acc | match ext {
                name if name == vk::KHR_VIDEO_ENCODE_H264_NAME => {
                    vk::VideoCodecOperationFlagsKHR::ENCODE_H264
                }
                name if name == vk::KHR_VIDEO_ENCODE_H265_NAME => {
                    vk::VideoCodecOperationFlagsKHR::ENCODE_H265
                }
                name if name == vk::KHR_VIDEO_DECODE_H264_NAME => {
                    vk::VideoCodecOperationFlagsKHR::DECODE_H264
                }
                name if name == vk::KHR_VIDEO_DECODE_H265_NAME => {
                    vk::VideoCodecOperationFlagsKHR::DECODE_H265
                }
                _ => vk::VideoCodecOperationFlagsKHR::empty(),
            }
        })
}

fn find_video_queue_idx(
    queues: &[vk::QueueFamilyProperties2<'_>],
    queue_flag: vk::QueueFlags,
    video_codec_operation: vk::VideoCodecOperationFlagsKHR,
) -> Option<usize> {
    for (i, queue) in queues.iter().enumerate() {
        if !queue
            .queue_family_properties
            .queue_flags
            .contains(queue_flag)
        {
            continue;
        }

        unsafe {
            find_ext!(queue, video_properties @ vk::QueueFamilyVideoPropertiesKHR =>
                if video_properties
                    .video_codec_operations
                    .contains(video_codec_operation)
                {
                    return Some(i);
                }
            );
        }
    }

    None
}

#[derive(Clone)]
pub struct VulkanAdapterInfo {
    pub name: String,
    pub driver_name: String,
    pub driver_info: String,
    pub device_type: vk::PhysicalDeviceType,
    pub supports_decoding: bool,
    pub supports_encoding: bool,
    pub device_properties: vk::PhysicalDeviceProperties,
    pub decode_capabilities: DecodeCapabilities,
    pub encode_capabilities: EncodeCapabilities,
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanAdapterInitError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Profile does not support NV12 texture format")]
    NoNV12ProfileSupport,
}
