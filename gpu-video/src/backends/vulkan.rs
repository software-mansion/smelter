pub(crate) mod codec;
pub(crate) mod vulkan_adapter;
pub(crate) mod vulkan_decoder;
pub(crate) mod vulkan_device;
pub(crate) mod vulkan_encoder;
pub(crate) mod vulkan_instance;
pub(crate) mod wrappers;

use std::sync::Arc;

pub use vulkan_adapter::{VulkanAdapter, VulkanAdapterInfo, VulkanAdapterInitError};
pub use vulkan_device::{VulkanDevice, VulkanDeviceInitError};
pub use vulkan_instance::{VulkanInstance, VulkanInstanceInitError};

use crate::{
    VideoInstanceInitError,
    backends::{CoreBackend, vulkan::wrappers::ImageKey},
    instance::{VideoInstanceBackend, VideoInstanceDescriptor},
};
use ash::vk;

pub struct VulkanBackend;

impl CoreBackend for VulkanBackend {
    fn new_instance(
        &self,
        desc: &VideoInstanceDescriptor,
    ) -> Result<Arc<dyn VideoInstanceBackend>, VideoInstanceInitError> {
        VulkanInstance::new(desc)
            .map(|instance| Arc::new(instance) as Arc<dyn VideoInstanceBackend>)
            .map_err(Into::into)
    }
}

#[cfg(feature = "wgpu")]
impl super::WgpuBackend for VulkanBackend {
    fn device_key_from_wgpu_device(
        &self,
        device: &wgpu::Device,
    ) -> crate::global_registry::VideoDeviceKey {
        use ash::vk::Handle;

        let hal_device = unsafe { device.as_hal::<wgpu::hal::vulkan::Api>().unwrap() };
        crate::global_registry::VideoDeviceKey::Vulkan {
            device_handle: hal_device.raw_device().handle().as_raw(),
            queue_handle: hal_device.raw_queue().as_raw(),
        }
    }

    fn retrieve_adapter_info(
        &self,
        wgpu_adapter: &wgpu::Adapter,
    ) -> Option<crate::adapter::VideoAdapterInfo> {
        use crate::adapter::VideoAdapterBackend;
        use vulkan_adapter::with_vulkan_adapter_from_wgpu;

        with_vulkan_adapter_from_wgpu(wgpu_adapter, |adapter| adapter.build_info())
    }

    fn create_and_register_device(
        &self,
        wgpu_adapter: &wgpu::Adapter,
        desc: &crate::device::VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), crate::VideoDeviceInitError> {
        use vulkan_adapter::with_vulkan_adapter_from_wgpu;
        with_vulkan_adapter_from_wgpu(wgpu_adapter, |vulkan_adapter| {
            VulkanDevice::create_and_register_wgpu(wgpu_adapter, vulkan_adapter, desc.clone())
                .map_err(Into::into)
        })
        .ok_or(crate::VideoDeviceInitError::NotSuitableAdapter)?
    }
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanCommonError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Cannot find a queue with index {0}")]
    NoQueue(usize),

    #[error("Memory copy requested to a buffer that is not set up for receiving input")]
    UploadToImproperBuffer,

    #[error("A slot in the Decoded Pictures Buffer was requested, but all slots are taken")]
    NoFreeSlotsInDpb,

    #[error("DPB can have at most 32 slots, {0} was requested")]
    DpbTooLong(u32),

    #[error("Tried to wait for an unsignaled semaphore value")]
    SemaphoreWaitOnUnsignaledValue,

    #[error("Tried to register {0:x?} as a new image, while it already exists")]
    RegisteredNewImageTwice(ImageKey),

    #[error("Tried to access state of image {0:x?}, which does not exist")]
    TriedToAccessNonexistentImageState(ImageKey),

    #[error("Tried to unregister image {0:x?} that was not registered")]
    UnregisteredNonexistentImage(ImageKey),

    #[error("Unsupported image aspect: {0:?}")]
    UnsupportedImageAspect(vk::ImageAspectFlags),

    #[error(
        "The reference image is smaller than the requested extent. Requested: {requested:?}, max allowed: {max_extent:?}"
    )]
    ReferenceImageTooSmall {
        requested: vk::Extent2D,
        max_extent: vk::Extent2D,
    },
}
