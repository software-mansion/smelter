use crate::graphics_context::wgpu_context::create_wgpu_graphics_ctx;
use std::sync::Arc;

#[cfg(feature = "vk-video")]
pub mod vulkan_context;
pub mod wgpu_context;

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

#[derive(Debug, Default, Clone)]
pub struct GraphicsContextOptions<'a> {
    pub device_id: Option<u32>,
    pub driver_name: Option<String>,
    pub force_gpu: bool,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
    pub compatible_surface: Option<&'a wgpu::Surface<'a>>,
    pub libvulkan_path: Option<&'a std::ffi::OsStr>,
}

impl GraphicsContext {
    #[cfg(feature = "vk-video")]
    pub fn new(opts: GraphicsContextOptions) -> Result<Self, CreateGraphicsContextError> {
        use crate::graphics_context::vulkan_context::create_vulkan_graphics_ctx;
        use tracing::warn;

        match create_vulkan_graphics_ctx(opts.clone()) {
            Err(err) => {
                warn!(
                    "Cannot initialize vulkan video context. Reason: {err}. Initializing without vulkan video support."
                );
                create_wgpu_graphics_ctx(opts)
            }
            ctx => ctx,
        }
    }

    #[cfg(not(feature = "vk-video"))]
    pub fn new(opts: GraphicsContextOptions) -> Result<Self, CreateGraphicsContextError> {
        create_wgpu_graphics_ctx(opts)
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

#[derive(Debug, thiserror::Error)]
pub enum CreateGraphicsContextError {
    #[error("Failed to get an adapter.")]
    NoAdapter,

    #[error("Error when requesting a wgpu adapter.")]
    RequestWgpuAdapterError(#[from] wgpu::RequestAdapterError),

    #[error("Failed to get a wgpu device.")]
    NoWgpuDevice(#[from] wgpu::RequestDeviceError),

    #[cfg(feature = "vk-video")]
    #[error(transparent)]
    VulkanInitError(#[from] vk_video::VulkanInitError),
}
