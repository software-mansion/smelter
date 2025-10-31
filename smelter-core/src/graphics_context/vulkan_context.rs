use itertools::Itertools;
use smelter_render::{required_wgpu_features, set_required_wgpu_limits};
use tracing::info;

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
        device_id,
        driver_name,
        ..
    } = opts;

    let vulkan_features = features | required_wgpu_features() | wgpu::Features::TEXTURE_FORMAT_NV12;

    let limits = set_required_wgpu_limits(limits);

    let instance = match libvulkan_path {
        Some(path) => vk_video::VulkanInstance::new_from(path),
        None => vk_video::VulkanInstance::new(),
    }?;

    log_available_adapters(&instance, compatible_surface)?;

    let adapter = instance
        .iter_adapters(compatible_surface)?
        .filter(|a| match device_id {
            Some(device_id) => a.info().device_properties.device_id == device_id,
            None => true,
        })
        .filter(|a| match driver_name {
            Some(ref driver_name) => {
                let info = a.info();
                let driver_name = driver_name.to_lowercase();
                info.driver_name.to_lowercase().contains(&driver_name)
                    || info.driver_info.to_lowercase().contains(&driver_name)
            }
            None => true,
        })
        .sorted_by_key(|a| {
            let video_based_priority = match (a.supports_decoding(), a.supports_encoding()) {
                (true, true) => 0,
                (true, false) | (false, true) => 1,
                (false, false) => 2,
            };
            let performance_based_priority = match a.info().device_type {
                wgpu::DeviceType::DiscreteGpu => 0,
                wgpu::DeviceType::IntegratedGpu => 1,
                _ => 3,
            };

            (performance_based_priority, video_based_priority)
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

fn log_available_adapters(
    instance: &vk_video::VulkanInstance,
    compatible_surface: Option<&wgpu::Surface>,
) -> Result<(), CreateGraphicsContextError> {
    let adapters: Vec<_> = instance
        .iter_adapters(compatible_surface)?
        .map(|adapter| {
            let info = adapter.info();
            format!("\n - {info:?}")
        })
        .collect();
    info!("Available adapters: {}", adapters.join(""));
    Ok(())
}
