use crate::error::InitPipelineError;
use smelter_render::{create_wgpu_ctx, error::InitRendererEngineError, WgpuComponents};
use std::sync::Arc;

#[cfg(feature = "vk-video")]
#[derive(Debug, Clone)]
pub struct VulkanCtx {
    pub device: Arc<vk_video::VulkanDevice>,
    pub instance: Arc<vk_video::VulkanInstance>,
}

#[derive(Debug, Clone)]
pub struct GraphicsContext {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub adapter: Arc<wgpu::Adapter>,
    pub instance: Arc<wgpu::Instance>,

    #[cfg(feature = "vk-video")]
    pub vulkan_ctx: Option<VulkanCtx>,
}

#[derive(Debug, Default)]
pub struct GraphicsContextOptions<'a> {
    pub device_id: Option<u32>,
    pub force_gpu: bool,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
    pub compatible_surface: Option<&'a wgpu::Surface<'a>>,
    pub libvulkan_path: Option<&'a std::ffi::OsStr>,
}

impl GraphicsContext {
    #[cfg(feature = "vk-video")]
    pub fn new(opts: GraphicsContextOptions) -> Result<Self, InitPipelineError> {
        use itertools::Itertools;
        use smelter_render::{required_wgpu_features, set_required_wgpu_limits};
        use tracing::warn;
        use vk_video::VulkanInitError;

        let GraphicsContextOptions {
            device_id,
            force_gpu,
            features,
            limits,
            compatible_surface,
            libvulkan_path,
        } = opts;

        let vulkan_features =
            features | required_wgpu_features() | wgpu::Features::TEXTURE_FORMAT_NV12;

        let limits = set_required_wgpu_limits(limits);

        let new_instance = || -> Result<_, VulkanInitError> {
            let instance = match libvulkan_path {
                Some(path) => vk_video::VulkanInstance::new_from(path),
                None => vk_video::VulkanInstance::new(),
            }?;

            log_available_adapters(&instance, compatible_surface);
            let adapter = match device_id {
                Some(device_id) => instance
                    .iter_adapters(compatible_surface)?
                    .find(|a| a.info().device_properties.device_id == device_id)
                    .ok_or(VulkanInitError::NoDevice)?,
                None => instance
                    .iter_adapters(compatible_surface)?
                    .sorted_by_key(|a| match (a.supports_decoding(), a.supports_encoding()) {
                        (true, true) => 0,
                        (true, false) | (false, true) => 1,
                        (false, false) => 2,
                    })
                    .next()
                    .ok_or(VulkanInitError::NoDevice)?,
            };

            let device = adapter.create_device(vulkan_features, limits.clone())?;
            Ok((instance, device))
        };

        match new_instance() {
            Ok((instance, device)) => Ok(GraphicsContext {
                device: device.wgpu_device().into(),
                queue: device.wgpu_queue().into(),
                adapter: device.wgpu_adapter().into(),
                instance: instance.wgpu_instance().into(),
                vulkan_ctx: Some(VulkanCtx { instance, device }),
            }),

            Err(err) => {
                warn!("Cannot initialize vulkan video decoding context. Reason: {err}. Initializing without vulkan video support.");

                let WgpuComponents {
                    instance,
                    adapter,
                    device,
                    queue,
                } = create_wgpu_ctx(device_id, force_gpu, features, limits, compatible_surface)
                    .map_err(InitRendererEngineError::FailedToInitWgpuCtx)?;

                Ok(GraphicsContext {
                    device,
                    queue,
                    adapter,
                    instance,
                    vulkan_ctx: None,
                })
            }
        }
    }

    #[cfg(not(feature = "vk-video"))]
    pub fn new(opts: GraphicsContextOptions) -> Result<Self, InitPipelineError> {
        let GraphicsContextOptions {
            device_id,
            force_gpu,
            features,
            limits,
            compatible_surface,
            ..
        } = opts;
        let WgpuComponents {
            instance,
            adapter,
            device,
            queue,
        } = create_wgpu_ctx(device_id, force_gpu, features, limits, compatible_surface)
            .map_err(InitRendererEngineError::FailedToInitWgpuCtx)?;

        Ok(GraphicsContext {
            device,
            queue,
            adapter,
            instance,
        })
    }

    #[cfg(feature = "vk-video")]
    pub fn has_vulkan_decoder_support(&self) -> bool {
        self.vulkan_ctx
            .as_ref()
            .map(|ctx| ctx.device.supports_decoding())
            .unwrap_or(false)
    }
    #[cfg(not(feature = "vk-video"))]
    pub fn has_vulkan_decoder_support(&self) -> bool {
        false
    }

    #[cfg(feature = "vk-video")]
    pub fn has_vulkan_encoder_support(&self) -> bool {
        self.vulkan_ctx
            .as_ref()
            .map(|ctx| ctx.device.supports_encoding())
            .unwrap_or(false)
    }
    #[cfg(not(feature = "vk-video"))]
    pub fn has_vulkan_encoder_support(&self) -> bool {
        false
    }
}

#[cfg(feature = "vk-video")]
fn log_available_adapters(
    instance: &vk_video::VulkanInstance,
    compatible_surface: Option<&wgpu::Surface<'_>>,
) {
    use tracing::info;

    let adapters = match instance.iter_adapters(compatible_surface) {
        Ok(adapters) => adapters
            .map(|adapter| format!("\n - {:?}", adapter.info()))
            .collect(),
        Err(_) => Vec::new(),
    };

    info!("Available adapters: {}", adapters.join(""))
}
