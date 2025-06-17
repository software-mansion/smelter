use log::error;

pub(crate) mod common_pipeline;
mod ctx;
pub(crate) mod format;
pub(crate) mod texture;
pub(crate) mod utils;

pub(crate) use ctx::WgpuCtx;
pub use ctx::{create_wgpu_ctx, required_wgpu_features, set_required_wgpu_limits, WgpuComponents};
pub use wgpu::Features as WgpuFeatures;

#[must_use]
pub(crate) struct WgpuErrorScope;

impl WgpuErrorScope {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn push(device: &wgpu::Device) -> Self {
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);

        Self
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn push(device: &wgpu::Device) -> Self {
        Self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pop(self, device: &wgpu::Device) -> Result<(), WgpuError> {
        for _ in 0..2 {
            if let Some(error) = pollster::block_on(device.pop_error_scope()) {
                return Err(error.into());
            }
        }

        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn pop(self, device: &wgpu::Device) -> Result<(), WgpuError> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum CreateWgpuCtxError {
    #[error("Failed to get a wgpu adapter.")]
    NoAdapter,

    #[error("Error when requesting a wgpu adapter.")]
    AdapterError(#[from] wgpu::RequestAdapterError),

    #[error("Failed to get a wgpu device.")]
    NoDevice(#[from] wgpu::RequestDeviceError),

    #[error(transparent)]
    WgpuError(#[from] WgpuError),
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum WgpuError {
    #[error("Wgpu validation error:\n{0}")]
    Validation(String),
    #[error("Wgpu out of memory error:\n{0}")]
    OutOfMemory(String),
    #[error("Wgpu internal error:\n{0}")]
    Internal(String),
}

/// Convert to custom error because wgpu::Error is not Send/Sync
impl From<wgpu::Error> for WgpuError {
    fn from(value: wgpu::Error) -> Self {
        match value {
            wgpu::Error::OutOfMemory { .. } => Self::OutOfMemory(value.to_string()),
            wgpu::Error::Validation { .. } => Self::Validation(value.to_string()),
            wgpu::Error::Internal { .. } => Self::Internal(value.to_string()),
        }
    }
}
