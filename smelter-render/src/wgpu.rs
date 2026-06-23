pub(crate) mod common_pipeline;
mod ctx;
pub(crate) mod format;
pub(crate) mod texture;
pub(crate) mod utils;

pub use ctx::WgpuCtx;
pub use ctx::{required_wgpu_features, set_required_wgpu_limits};
pub use wgpu::Features as WgpuFeatures;

#[must_use]
pub(crate) struct WgpuErrorScope {
    #[cfg(not(target_arch = "wasm32"))]
    scopes: Option<[wgpu::ErrorScopeGuard; 2]>,
}

impl WgpuErrorScope {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn push(device: &wgpu::Device) -> Self {
        let scopes = [
            device.push_error_scope(wgpu::ErrorFilter::Validation),
            device.push_error_scope(wgpu::ErrorFilter::OutOfMemory),
        ];
        Self { scopes: Some(scopes) }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn push(_device: &wgpu::Device) -> Self {
        Self {}
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pop(mut self) -> Result<(), WgpuError> {
        let scopes = self.scopes.take().expect("WgpuErrorScope::pop called twice");
        for scope in scopes.into_iter().rev() {
            if let Some(error) = pollster::block_on(scope.pop()) {
                return Err(error.into());
            }
        }

        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn pop(self) -> Result<(), WgpuError> {
        Ok(())
    }
}

impl Drop for WgpuErrorScope {
    #[cfg(not(target_arch = "wasm32"))]
    fn drop(&mut self) {
        let Some(scopes) = self.scopes.take() else {
            return;
        };
        for scope in scopes.into_iter().rev() {
            if let Some(err) = pollster::block_on(scope.pop()) {
                tracing::error!("Wgpu error in dropped scope: {err}");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn drop(&mut self) {}
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum CreateWgpuCtxError {
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
