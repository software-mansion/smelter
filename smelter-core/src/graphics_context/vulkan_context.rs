use itertools::Itertools;
use smelter_render::{required_wgpu_features, set_required_wgpu_limits};

use crate::graphics_context::{
    CreateGraphicsContextError, GraphicsContext, GraphicsContextOptions, VulkanCtx,
};

pub fn create_vulkan_graphics_ctx(
    opts: GraphicsContextOptions,
) -> Result<GraphicsContext, CreateGraphicsContextError> {
    let GraphicsContextOptions {
        features,
        limits,
        compatible_surface,
        libvulkan_path,
        ..
    } = opts;

    let vulkan_features = features | required_wgpu_features() | wgpu::Features::TEXTURE_FORMAT_NV12;

    let limits = set_required_wgpu_limits(limits);

    let instance = match libvulkan_path {
        Some(path) => vk_video::VulkanInstance::new_from(path),
        None => vk_video::VulkanInstance::new(),
    }?;
    let adapter = instance
        .iter_adapters(compatible_surface)?
        .sorted_by_key(|a| match (a.supports_decoding(), a.supports_encoding()) {
            (true, true) => 0,
            (true, false) | (false, true) => 1,
            (false, false) => 2,
        })
        .next()
        .ok_or(CreateGraphicsContextError::NoAdapter)?;
    let device = adapter.create_device(vulkan_features, limits.clone())?;

    Ok(GraphicsContext {
        device: device.wgpu_device().into(),
        queue: device.wgpu_queue().into(),
        adapter: device.wgpu_adapter().into(),
        instance: instance.wgpu_instance().into(),
        vulkan_ctx: Some(VulkanCtx { instance, device }),
    })
}
