use std::{
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    sync::Arc,
};

use wgpu::hal::api::Vulkan as VkApi;

use super::{
    DmaBufInterop,
    interop::VulkanDmaBufDevice,
    semaphore::{VulkanSemaphore, VulkanSemaphoreError},
    sync_file,
};

#[derive(Clone)]
pub(crate) struct DmaBufSyncFd {
    fd: Arc<OwnedFd>,
}

impl DmaBufSyncFd {
    pub(crate) fn new(fd: OwnedFd) -> Self {
        Self { fd: Arc::new(fd) }
    }

    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_ref().as_fd()
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum QuickSyncDmaBufSyncError {
    #[error("failed to import DMA-BUF sync_file: {0}")]
    ImportSyncFile(#[source] std::io::Error),

    #[error(transparent)]
    VulkanSemaphore(#[from] VulkanSemaphoreError),

    #[error("DMA-BUF sync requires a Vulkan wgpu queue")]
    MissingVulkanQueue,
}

/// Attaches write fences to DMA-BUF frames so external consumers (oneVPL and
/// the media engine) wait for wgpu work.
///
/// This is the only direction that needs a GPU fence: every
/// consumer-to-producer edge in the QuickSync pipeline is already ordered by
/// a oneVPL CPU sync point before the shared surface is touched again.
pub(crate) struct QuickSyncDmaBufSync {
    queue: wgpu::Queue,
    vulkan: Arc<VulkanDmaBufDevice>,
}

impl QuickSyncDmaBufSync {
    pub(crate) fn new(interop: &DmaBufInterop, queue: &wgpu::Queue) -> Self {
        Self {
            queue: queue.clone(),
            vulkan: Arc::clone(&interop.vulkan),
        }
    }

    /// Submits `command_buffers` and attaches a write fence to `frame` that
    /// signals once they, and everything submitted before them, finished.
    ///
    /// Queue-staged semaphores attach to whichever submit reaches the queue
    /// next, so a concurrent submit may adopt the release semaphore instead
    /// of the flush submit below. That is sound: the semaphore is staged only
    /// after `command_buffers` were submitted, and wgpu executes submissions
    /// strictly in order, so any adopter completes at or after the frame's
    /// work. Staging before the frame submit would allow the opposite —
    /// adoption by an earlier submit, fencing the frame before it is written.
    pub(crate) fn submit_frame_write(
        &self,
        frame: &DmaBufSyncFd,
        command_buffers: impl IntoIterator<Item = wgpu::CommandBuffer>,
    ) -> Result<(), QuickSyncDmaBufSyncError> {
        self.queue.submit(command_buffers);

        let release = VulkanSemaphore::exportable(Arc::clone(&self.vulkan))?;
        {
            let hal_queue = unsafe {
                self.queue
                    .as_hal::<VkApi>()
                    .ok_or(QuickSyncDmaBufSyncError::MissingVulkanQueue)?
            };
            hal_queue.add_signal_semaphore(release.raw(), None);
        }
        self.queue.submit([]);

        let result = release
            .export_sync_file()
            .map_err(QuickSyncDmaBufSyncError::from)
            .and_then(|sync_file| {
                sync_file::import_write_fence(frame.as_fd(), &sync_file)
                    .map_err(QuickSyncDmaBufSyncError::ImportSyncFile)
            });
        let frame = frame.clone();
        self.queue
            .on_submitted_work_done(move || drop((frame, release)));
        result
    }
}
