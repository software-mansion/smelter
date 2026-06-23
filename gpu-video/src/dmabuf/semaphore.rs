use std::{
    os::fd::{AsRawFd, IntoRawFd, OwnedFd},
    sync::Arc,
};

use ash::vk;

use super::{interop::VulkanDmaBufDevice, sync_file::SyncFile};

#[derive(Debug, thiserror::Error)]
pub(crate) enum VulkanSemaphoreError {
    #[error("failed to create Vulkan semaphore: {0}")]
    Create(vk::Result),

    #[error("failed to import sync_file as Vulkan semaphore: {0}")]
    Import(vk::Result),

    #[error("failed to export Vulkan semaphore as sync_file: {0}")]
    Export(vk::Result),
}

pub(super) struct VulkanSemaphore {
    semaphore: vk::Semaphore,
    vulkan: Arc<VulkanDmaBufDevice>,
}

impl VulkanSemaphore {
    pub(super) fn import_sync_file(
        vulkan: Arc<VulkanDmaBufDevice>,
        fd: OwnedFd,
    ) -> Result<Self, VulkanSemaphoreError> {
        let semaphore = Self::create(Arc::clone(&vulkan))?;
        let import_info = vk::ImportSemaphoreFdInfoKHR::default()
            .semaphore(semaphore.semaphore)
            .flags(vk::SemaphoreImportFlags::TEMPORARY)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
            .fd(fd.as_raw_fd());

        match unsafe { vulkan.external_semaphore_fd.import_semaphore_fd(&import_info) } {
            Ok(()) => {
                let _ = fd.into_raw_fd();
                Ok(semaphore)
            }
            Err(err) => {
                drop(semaphore);
                Err(VulkanSemaphoreError::Import(err))
            }
        }
    }

    pub(super) fn exportable(
        vulkan: Arc<VulkanDmaBufDevice>,
    ) -> Result<Self, VulkanSemaphoreError> {
        let mut export_info = vk::ExportSemaphoreCreateInfo::default()
            .handle_types(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        let create_info = vk::SemaphoreCreateInfo::default().push_next(&mut export_info);
        Self::create_with_info(vulkan, &create_info)
    }

    pub(super) fn export_sync_file(&self) -> Result<SyncFile, VulkanSemaphoreError> {
        let get_fd_info = vk::SemaphoreGetFdInfoKHR::default()
            .semaphore(self.semaphore)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        let fd =
            unsafe { self.vulkan.external_semaphore_fd.get_semaphore_fd(&get_fd_info) }
                .map_err(VulkanSemaphoreError::Export)?;

        Ok(SyncFile::from_owned_raw_fd(fd))
    }

    pub(super) fn raw(&self) -> vk::Semaphore {
        self.semaphore
    }

    fn create(vulkan: Arc<VulkanDmaBufDevice>) -> Result<Self, VulkanSemaphoreError> {
        Self::create_with_info(vulkan, &vk::SemaphoreCreateInfo::default())
    }

    fn create_with_info(
        vulkan: Arc<VulkanDmaBufDevice>,
        create_info: &vk::SemaphoreCreateInfo<'_>,
    ) -> Result<Self, VulkanSemaphoreError> {
        let semaphore = unsafe { vulkan.device.create_semaphore(create_info, None) }
            .map_err(VulkanSemaphoreError::Create)?;

        Ok(Self { semaphore, vulkan })
    }
}

impl Drop for VulkanSemaphore {
    fn drop(&mut self) {
        unsafe {
            self.vulkan.device.destroy_semaphore(self.semaphore, None);
        }
    }
}
