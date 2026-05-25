use std::sync::Arc;

use ash::{Entry, vk};
use gpu_video::{
    VideoAdapterExt, VideoInstance,
    capabilities::VulkanDeviceType,
    parameters::{VideoDeviceDescriptor, VideoInstanceDescriptor},
};
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

    let instance_flags = wgpu::InstanceFlags::default();
    let api_version = vk::API_VERSION_1_3;
    let wgpu_features = features | required_wgpu_features() | wgpu::Features::TEXTURE_FORMAT_NV12;

    let limits = set_required_wgpu_limits(limits);

    let entry = match libvulkan_path {
        Some(lib_path) => Arc::new(unsafe { Entry::load_from(lib_path)? }),
        None => Arc::new(unsafe { Entry::load()? }),
    };
    let mut extensions =
        wgpu::hal::vulkan::Instance::desired_extensions(&entry, api_version, instance_flags)?;

    let video_instance = VideoInstance::new_from_entry(
        entry.clone(),
        &mut extensions,
        &VideoInstanceDescriptor {
            enable_validations: instance_flags
                .intersects(wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::DEBUG),
            enable_api_dump: false,
        },
    )?;

    let hal_instance = unsafe {
        wgpu::hal::vulkan::Instance::from_raw(
            entry.as_ref().clone(),
            video_instance.raw_instance(),
            api_version,
            0,
            None,
            extensions,
            instance_flags,
            wgpu::MemoryBudgetThresholds::default(),
            false,
            Some(Box::new(move || drop(video_instance))),
        )?
    };
    let instance = unsafe { wgpu::Instance::from_hal::<wgpu::hal::vulkan::Api>(hal_instance) };

    log_available_adapters(&instance);

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

fn log_available_adapters(instance: &wgpu::Instance) {
    let adapters: Vec<_> = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN))
        .iter()
        .filter_map(|adapter| {
            let info = adapter.video_adapter_info()?;
            Some(format!("\n - {info:?}"))
        })
        .collect();
    info!("Available adapters: {}", adapters.join(""));
}
