mod interop;
mod semaphore;
mod sync;
mod sync_file;

use std::{ffi::CStr, fmt, os::fd::OwnedFd, sync::Arc};

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
    pub(crate) size: u32,
    pub(crate) modifier: u64,
}

impl fmt::Debug for DmaBufObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DMA-BUF object")
            .field("size", &self.size)
            .field("modifier", &self.modifier)
            .finish()
    }
}

pub(crate) fn required_wgpu_features() -> wgpu::Features {
    wgpu::Features::VULKAN_EXTERNAL_MEMORY_FD
        | wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF
}

pub(crate) const REQUIRED_VULKAN_DEVICE_EXTENSIONS: [&CStr; 5] = [
    ash::khr::external_memory_fd::NAME,
    ash::ext::external_memory_dma_buf::NAME,
    ash::ext::image_drm_format_modifier::NAME,
    ash::khr::external_semaphore::NAME,
    ash::khr::external_semaphore_fd::NAME,
];

pub(crate) fn missing_required_vulkan_device_extension(
    supports: impl Fn(&CStr) -> bool,
) -> Option<&'static CStr> {
    REQUIRED_VULKAN_DEVICE_EXTENSIONS.into_iter().find(|extension| !supports(extension))
}
