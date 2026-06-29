use ash::vk;
use wgpu::hal::vulkan::Api as VkApi;

use crate::{backends::vulkan::VulkanInstance, instance::VideoInstanceDescriptor};

use super::VulkanAdapter;

pub(crate) fn with_video_adapter_from_wgpu<F, R>(
    wgpu_adapter: &wgpu::Adapter,
    use_adapter: F,
) -> Option<R>
where
    F: Fn(VulkanAdapter<'_>) -> R,
{
    let hal_adapter = unsafe { wgpu_adapter.as_hal::<VkApi>()? };
    let physical_device = hal_adapter.raw_physical_device();
    let instance = hal_adapter.shared_instance();
    let instance = VulkanInstance::new_unowned(
        instance.raw_instance().clone(),
        instance.entry().clone(),
        &VideoInstanceDescriptor {
            enable_validations: instance.extensions().contains(&vk::EXT_DEBUG_UTILS_NAME),
            ..Default::default()
        },
    );

    VulkanAdapter::new(&instance, physical_device).map(use_adapter)
}
