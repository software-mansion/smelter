mod interop;
mod nv12;
mod semaphore;
mod sync;
mod sync_file;
mod vulkan;

use std::ffi::CStr;

pub(crate) use interop::DmaBufInterop;
pub(crate) use nv12::{
    DRM_FORMAT_NV12, DmaBufError, DmaBufFrame, DmaBufObject, DmaBufPlane, Nv12DmaBufDescriptor,
    Nv12DmaBufLayer,
};
pub(crate) use sync::QuickSyncDmaBufSync;

pub(crate) fn required_wgpu_features() -> wgpu::Features {
    wgpu::Features::TEXTURE_FORMAT_NV12
        | wgpu::Features::VULKAN_EXTERNAL_MEMORY_FD
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
    REQUIRED_VULKAN_DEVICE_EXTENSIONS
        .into_iter()
        .find(|extension| !supports(extension))
}
