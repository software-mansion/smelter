use ash::vk;
use std::{ffi::CStr, sync::Arc};
use tracing::{debug, warn};
use wgpu::hal::DynAdapter;

use crate::{
    device::{
        caps::{DecodeCapabilities, NativeEncodeCapabilities},
        queues::{QueueIndex, QueueIndices},
        DECODE_EXTENSIONS, ENCODE_EXTENSIONS, REQUIRED_EXTENSIONS,
    },
    VulkanDevice, VulkanInitError, VulkanInstance,
};

/// Represents handle to a physical device.
/// Can be used to create [`VulkanDevice`].
pub struct VulkanAdapter<'a> {
    pub(crate) instance: &'a VulkanInstance,
    pub(crate) device_candidate: DeviceCandidate,
    pub(crate) info: AdapterInfo,
}

impl<'a> VulkanAdapter<'a> {
    fn new(
        vulkan_instance: &'a VulkanInstance,
        compatible_surface: Option<&'a wgpu::Surface<'_>>,
        device: vk::PhysicalDevice,
    ) -> Option<Self> {
        let instance = &vulkan_instance.instance;
        let wgpu_instance = &vulkan_instance.wgpu_instance;
        let wgpu_instance = unsafe { wgpu_instance.as_hal::<wgpu::hal::vulkan::Api>() }.unwrap();

        let properties = unsafe { instance.get_physical_device_properties(device) };
        let device_name = properties
            .device_name_as_c_str()
            .map(CStr::to_string_lossy)
            .unwrap_or("unknown".into());

        let wgpu_adapter = wgpu_instance.expose_adapter(device)?;

        if let Some(surface) = compatible_surface {
            unsafe {
                (*surface).as_hal::<wgpu::hal::vulkan::Api, _, _>(|surface| {
                    surface.and_then(|surface| wgpu_adapter.adapter.surface_capabilities(surface))
                })?
            };
        }

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

        if vk_13_features.synchronization2 == 0 {
            warn!("device {device_name} does not support the required synchronization2 feature",);
        }

        if !contains_extensions(REQUIRED_EXTENSIONS, &extensions) {
            warn!("device {device_name} does not support the required extensions",);
            return None;
        }

        let has_decode_extensions = contains_extensions(DECODE_EXTENSIONS, &extensions);
        let has_encode_extensions = contains_extensions(ENCODE_EXTENSIONS, &extensions);
        if !has_decode_extensions && !has_encode_extensions {
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

        let decode_capabilities = match has_decode_extensions {
            true => match DecodeCapabilities::query(instance, device) {
                Ok(caps) => caps,
                Err(err) => {
                    warn!("Couldn't query device decode capabilities: {err}");
                    return None;
                }
            },
            false => None,
        };

        let encode_capabilities = match has_encode_extensions {
            true => match NativeEncodeCapabilities::query(instance, device) {
                Ok(caps) => Some(caps),
                Err(err) => {
                    warn!("Couldn't query device encode capabilities: {err}");
                    return None;
                }
            },
            false => None,
        };

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

        let decode_queue_idx = match has_decode_extensions {
            true => Some(find_video_queue_idx(
                &queues,
                vk::QueueFlags::VIDEO_DECODE_KHR,
                vk::VideoCodecOperationFlagsKHR::DECODE_H264,
            )?),
            false => None,
        };
        let encode_queue_idx = match has_encode_extensions {
            true => Some(find_video_queue_idx(
                &queues,
                vk::QueueFlags::VIDEO_ENCODE_KHR,
                vk::VideoCodecOperationFlagsKHR::ENCODE_H264,
            )?),
            false => None,
        };

        debug!("decode capabilities: {decode_capabilities:#?}");
        debug!("encode capabilities: {encode_capabilities:#?}");

        let device_candidate = DeviceCandidate {
            physical_device: device,
            wgpu_adapter,
            queue_indices: QueueIndices {
                transfer: QueueIndex {
                    idx: transfer_queue_idx,
                    video_properties: video_properties[transfer_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [transfer_queue_idx],
                },
                h264_decode: decode_queue_idx.map(|idx| QueueIndex {
                    idx,
                    video_properties: video_properties[idx],
                    query_result_status_properties: query_result_status_properties[idx],
                }),
                h264_encode: encode_queue_idx.map(|idx| QueueIndex {
                    idx,
                    video_properties: video_properties[idx],
                    query_result_status_properties: query_result_status_properties[idx],
                }),
                graphics_transfer_compute: QueueIndex {
                    idx: graphics_transfer_compute_queue_idx,
                    video_properties: video_properties[graphics_transfer_compute_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [graphics_transfer_compute_queue_idx],
                },
            },
            decode_capabilities,
            encode_capabilities,
        };

        Some(Self {
            instance: vulkan_instance,
            device_candidate,
            info: AdapterInfo {
                device_properties: properties,
                supports_decoding: has_decode_extensions,
                supports_encoding: has_encode_extensions,
            },
        })
    }

    pub fn supports_decoding(&self) -> bool {
        self.info.supports_decoding
    }

    pub fn supports_encoding(&self) -> bool {
        self.info.supports_encoding
    }

    pub fn create_device(
        self,
        wgpu_features: wgpu::Features,
        wgpu_limits: wgpu::Limits,
    ) -> Result<Arc<VulkanDevice>, VulkanInitError> {
        Ok(VulkanDevice::new(self.instance, wgpu_features, wgpu_limits, self)?.into())
    }

    pub fn info(&self) -> &AdapterInfo {
        &self.info
    }
}

pub struct AdapterInfo {
    pub device_properties: vk::PhysicalDeviceProperties,
    pub supports_decoding: bool,
    pub supports_encoding: bool,
}

pub(crate) struct DeviceCandidate {
    pub(crate) physical_device: vk::PhysicalDevice,
    pub(crate) wgpu_adapter: wgpu::hal::ExposedAdapter<wgpu::hal::vulkan::Api>,
    pub(crate) queue_indices: QueueIndices<'static>,
    pub(crate) decode_capabilities: Option<DecodeCapabilities>,
    pub(crate) encode_capabilities: Option<NativeEncodeCapabilities>,
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

pub(crate) fn iter_adapters<'a>(
    vulkan_instance: &'a VulkanInstance,
    compatible_surface: Option<&'a wgpu::Surface<'_>>,
) -> Result<impl Iterator<Item = VulkanAdapter<'a>> + 'a, VulkanInitError> {
    let physical_devices = unsafe { vulkan_instance.instance.enumerate_physical_devices()? };
    Ok(physical_devices
        .into_iter()
        .filter_map(move |device| VulkanAdapter::new(vulkan_instance, compatible_surface, device)))
}

fn contains_extensions(
    required_extensions: &[&CStr],
    available_extensions: &[vk::ExtensionProperties],
) -> bool {
    required_extensions.iter().all(|&extension_name| {
        available_extensions.iter().any(|ext| {
            let Ok(name) = ext.extension_name_as_c_str() else {
                return false;
            };

            if name != extension_name {
                return false;
            };

            true
        })
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
