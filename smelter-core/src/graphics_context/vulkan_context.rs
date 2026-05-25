use gpu_video::{
    VideoAdapterExt, capabilities::VulkanDeviceType, parameters::VideoDeviceDescriptor,
};
use itertools::Itertools;
use smelter_render::{required_wgpu_features, set_required_wgpu_limits};
use tracing::info;

use crate::graphics_context::{
    CreateGraphicsContextError, GraphicsContext, GraphicsContextOptions, VulkanCtx,
};

// TODO: this should be renamed or maybe it should be somewhat merged with wgpu_context initalization,
// now that gpu-video's initialization code is similar to wgpu's
pub fn create_vulkan_graphics_ctx(
    opts: GraphicsContextOptions,
) -> Result<GraphicsContext, CreateGraphicsContextError> {
    let GraphicsContextOptions {
        features,
        limits,
        compatible_surface,
        // libvulkan_path,
        device_id,
        driver_name,
        display_handle,
        ..
    } = opts;

    let wgpu_features = features | required_wgpu_features() | wgpu::Features::TEXTURE_FORMAT_NV12;

    let limits = set_required_wgpu_limits(limits);

    // let instance = match libvulkan_path {
    //     Some(path) => gpu_video::VideoInstance::new_from(path),
    //     None => gpu_video::VideoInstance::new(),
    // }?;

    // TODO: This does not support providing libvulkan_path like we did in the previous version
    let instance_desc = match display_handle {
        Some(display_handle) => wgpu::InstanceDescriptor::new_with_display_handle(
            display_handle.to_boxed_display_handle(),
        ),
        None => wgpu::InstanceDescriptor::new_without_display_handle(),
    };
    let instance = wgpu::Instance::new(instance_desc);

    log_available_adapters(&instance)?;

    let (adapter, adapter_info) =
        pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN))
            .into_iter()
            .filter(|adapter| match compatible_surface {
                Some(surface) => adapter.is_surface_supported(surface),
                None => true,
            })
            .filter(|a| match device_id {
                Some(device_id) => a.get_info().device == device_id,
                None => true,
            })
            .filter(|a| match driver_name {
                Some(ref driver_name) => {
                    let info = a.get_info();
                    let driver_name = driver_name.to_lowercase();
                    info.driver.to_lowercase().contains(&driver_name)
                        || info.driver_info.to_lowercase().contains(&driver_name)
                }
                None => true,
            })
            .filter_map(|a| {
                let info = a.video_adapter_info()?;
                Some((a, info))
            })
            .sorted_by_key(|(_, info)| {
                let video_based_priority = match (info.supports_decoding, info.supports_encoding) {
                    (true, true) => 0,
                    (true, false) | (false, true) => 1,
                    (false, false) => 2,
                };
                let performance_based_priority = match info.device_type {
                    VulkanDeviceType::DISCRETE_GPU => 0,
                    VulkanDeviceType::INTEGRATED_GPU => 1,
                    _ => 3,
                };

                (performance_based_priority, video_based_priority)
            })
            .next()
            .ok_or(CreateGraphicsContextError::NoAdapter)?;

    info!("Using {} adapter with Vulkan backend", adapter_info.name);
    let (device, queue) = adapter.request_device_with_video_support(&VideoDeviceDescriptor {
        wgpu_features,
        wgpu_experimental_features: unsafe { wgpu::ExperimentalFeatures::enabled() },
        wgpu_limits: limits.clone(),
    })?;

    Ok(GraphicsContext {
        device: device.into(),
        queue: queue.into(),
        adapter: adapter.into(),
        instance: instance.into(),
        vulkan_ctx: Some(VulkanCtx {
            adapter_info: adapter_info.into(),
        }),
    })
}

fn log_available_adapters(instance: &wgpu::Instance) -> Result<(), CreateGraphicsContextError> {
    let adapters: Vec<_> = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN))
        .iter()
        .filter_map(|adapter| {
            let info = adapter.video_adapter_info()?;
            Some(format!("\n - {info:?}"))
        })
        .collect();
    info!("Available adapters: {}", adapters.join(""));
    Ok(())
}
