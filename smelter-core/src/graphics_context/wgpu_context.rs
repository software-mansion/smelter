use itertools::Itertools;
use smelter_render::{required_wgpu_features, set_required_wgpu_limits};
use tracing::{error, info};

use crate::graphics_context::{
    CreateGraphicsContextError, GraphicsContext, GraphicsContextOptions,
};

pub fn create_wgpu_graphics_ctx(
    opts: GraphicsContextOptions,
) -> Result<GraphicsContext, CreateGraphicsContextError> {
    let GraphicsContextOptions {
        force_gpu,
        features,
        limits,
        compatible_surface,
        device_id,
        driver_name,
        ..
    } = opts;

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    #[cfg(not(target_arch = "wasm32"))]
    log_available_adapters(&instance, compatible_surface);

    let adapter = instance
        .enumerate_adapters(wgpu::Backends::all())
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
        .sorted_by_key(|a| match a.get_info().device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            _ => 3,
        })
        .next()
        .ok_or(CreateGraphicsContextError::NoAdapter)?;

    let adapter_info = adapter.get_info();
    info!(
        "Using {} adapter with {:?} backend",
        adapter_info.name, adapter_info.backend
    );
    if force_gpu && adapter_info.device_type == wgpu::DeviceType::Cpu {
        error!("Selected adapter is CPU based. Aborting.");
        return Err(CreateGraphicsContextError::NoAdapter);
    }
    let required_features = features | required_wgpu_features();

    let missing_features = required_features.difference(adapter.features());
    if !missing_features.is_empty() {
        error!(
            "Selected adapter or its driver does not support required wgpu features. Missing features: {missing_features:?})."
        );
        error!(
            "You can configure some of the required features using \"SMELTER_REQUIRED_WGPU_FEATURES\" environment variable. Check https://smelter.dev/docs for more."
        );
        return Err(CreateGraphicsContextError::NoAdapter);
    }

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: None,
        required_limits: set_required_wgpu_limits(limits),
        required_features,
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
    }))?;

    Ok(GraphicsContext {
        device: device.into(),
        queue: queue.into(),
        adapter: adapter.into(),
        instance: instance.into(),
        #[cfg(feature = "vk-video")]
        vulkan_ctx: None,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn log_available_adapters(instance: &wgpu::Instance, compatible_surface: Option<&wgpu::Surface>) {
    let adapters: Vec<_> = instance
        .enumerate_adapters(wgpu::Backends::all())
        .iter()
        .filter(|adapter| match compatible_surface {
            Some(surface) => adapter.is_surface_supported(surface),
            None => true,
        })
        .map(|adapter| {
            let info = adapter.get_info();
            format!("\n - {info:?}")
        })
        .collect();
    info!("Available adapters: {}", adapters.join(""))
}
