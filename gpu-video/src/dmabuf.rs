mod interop;
mod semaphore;
mod sync;
mod sync_file;

use std::{ffi::CStr, os::fd::OwnedFd, sync::Arc};

pub(crate) use interop::DmaBufInterop;
pub(crate) use sync::{DmaBufSyncTarget, QuickSyncDmaBufSync};

#[derive(Debug, thiserror::Error)]
pub(crate) enum DmaBufError {
    #[error("unsupported DMA-BUF device: {0}")]
    UnsupportedDevice(String),
}

#[derive(Clone)]
pub(crate) struct DmaBufObject {
    pub(crate) fd: Arc<OwnedFd>,
}

pub(crate) fn required_wgpu_features() -> wgpu::Features {
    wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF
}

pub(crate) const REQUIRED_SYNC_VULKAN_DEVICE_EXTENSIONS: [&CStr; 2] = [
    ash::khr::external_semaphore::NAME,
    ash::khr::external_semaphore_fd::NAME,
];

pub(crate) fn missing_required_sync_vulkan_device_extension(
    supports: impl Fn(&CStr) -> bool,
) -> Option<&'static CStr> {
    REQUIRED_SYNC_VULKAN_DEVICE_EXTENSIONS
        .into_iter()
        .find(|extension| !supports(extension))
}
