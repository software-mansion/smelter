use std::{
    os::fd::AsFd,
    sync::{Arc, MutexGuard},
};

use ash::vk;
use wgpu::hal::api::Vulkan as VkApi;

use super::{
    DmaBufInterop,
    interop::VulkanDmaBufDevice,
    semaphore::{VulkanSemaphore, VulkanSemaphoreError},
    sync_file::{self, DmaBufAccess, SyncFile},
};

pub(crate) trait DmaBufSyncTarget: Clone + Send + 'static {
    fn objects(&self) -> &[super::DmaBufObject];
    fn sync_guard(&self) -> MutexGuard<'_, ()>;
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum QuickSyncDmaBufSyncError {
    #[error("failed to export DMA-BUF sync_file: {0}")]
    ExportSyncFile(#[source] std::io::Error),

    #[error("failed to import DMA-BUF sync_file: {0}")]
    ImportSyncFile(#[source] std::io::Error),

    #[error(transparent)]
    VulkanSemaphore(#[from] VulkanSemaphoreError),

    #[error("DMA-BUF sync requires a Vulkan wgpu queue")]
    MissingVulkanQueue,

    #[error("{label}: {source}")]
    Labeled { label: &'static str, source: Box<Self> },
}

impl QuickSyncDmaBufSyncError {
    fn labeled(self, label: &'static str) -> Self {
        Self::Labeled { label, source: Box::new(self) }
    }
}

fn label_error<E>(label: &'static str) -> impl FnOnce(E) -> QuickSyncDmaBufSyncError
where
    E: Into<QuickSyncDmaBufSyncError>,
{
    move |err| err.into().labeled(label)
}

pub(crate) struct QuickSyncDmaBufSync {
    queue: wgpu::Queue,
    vulkan: Arc<VulkanDmaBufDevice>,
}

impl QuickSyncDmaBufSync {
    pub(crate) fn new(interop: &DmaBufInterop, queue: &wgpu::Queue) -> Self {
        Self { queue: queue.clone(), vulkan: Arc::clone(&interop.vulkan) }
    }

    pub(crate) fn submit_target_write<T: DmaBufSyncTarget>(
        &self,
        target: &T,
        encoder: wgpu::CommandEncoder,
        label: &'static str,
    ) -> Result<(), QuickSyncDmaBufSyncError> {
        self.submit_dma_buf_access(target, DmaBufAccess::Write, [encoder.finish()], label)
    }

    pub(crate) fn submit_target_read<T: DmaBufSyncTarget>(
        &self,
        target: &T,
        encoder: wgpu::CommandEncoder,
        label: &'static str,
    ) -> Result<(), QuickSyncDmaBufSyncError> {
        self.submit_dma_buf_access(target, DmaBufAccess::Read, [encoder.finish()], label)
    }

    fn submit_dma_buf_access<T: DmaBufSyncTarget>(
        &self,
        frame: &T,
        access: DmaBufAccess,
        command_buffers: impl IntoIterator<Item = wgpu::CommandBuffer>,
        label: &'static str,
    ) -> Result<(), QuickSyncDmaBufSyncError> {
        let submitted_frame = frame.clone();
        let sync_guard = frame.sync_guard();
        let acquired = self.acquire_frame(frame, access).map_err(label_error(label))?;
        let release = VulkanSemaphore::exportable(Arc::clone(&self.vulkan))
            .map_err(label_error(label))?;
        let staged_submission = self
            .stage_submission_sync(&acquired, release.raw())
            .map_err(label_error(label))?;

        self.queue.submit(command_buffers);
        staged_submission.consume();

        let release_result = release
            .export_sync_file()
            .map_err(QuickSyncDmaBufSyncError::from)
            .and_then(|sync_file| self.release_frame(frame, access, &sync_file))
            .map_err(label_error(label));
        drop(sync_guard);
        self.queue
            .on_submitted_work_done(move || drop((submitted_frame, acquired, release)));
        release_result
    }

    fn acquire_frame<T: DmaBufSyncTarget>(
        &self,
        frame: &T,
        access: DmaBufAccess,
    ) -> Result<Box<[VulkanSemaphore]>, QuickSyncDmaBufSyncError> {
        let mut semaphores = Vec::with_capacity(frame.objects().len());
        for object in frame.objects() {
            let sync_file =
                sync_file::export_sync_file(object.fd.as_ref().as_fd(), access)
                    .map_err(QuickSyncDmaBufSyncError::ExportSyncFile)?;
            if let SyncFile::Pending(sync_file) = sync_file {
                semaphores.push(VulkanSemaphore::import_sync_file(
                    Arc::clone(&self.vulkan),
                    sync_file,
                )?);
            }
        }
        Ok(semaphores.into_boxed_slice())
    }

    fn release_frame<T: DmaBufSyncTarget>(
        &self,
        frame: &T,
        access: DmaBufAccess,
        sync_file: &SyncFile,
    ) -> Result<(), QuickSyncDmaBufSyncError> {
        for object in frame.objects() {
            sync_file::import_sync_file(object.fd.as_ref().as_fd(), access, sync_file)
                .map_err(QuickSyncDmaBufSyncError::ImportSyncFile)?;
        }
        Ok(())
    }

    fn stage_submission_sync(
        &self,
        acquire: &[VulkanSemaphore],
        release: vk::Semaphore,
    ) -> Result<StagedSubmissionSync, QuickSyncDmaBufSyncError> {
        let hal_queue = unsafe {
            self.queue
                .as_hal::<VkApi>()
                .ok_or(QuickSyncDmaBufSyncError::MissingVulkanQueue)?
        };
        let mut waits = Vec::with_capacity(acquire.len());
        for semaphore in acquire {
            hal_queue.add_wait_semaphore(
                semaphore.raw(),
                None,
                vk::PipelineStageFlags::ALL_COMMANDS,
            );
            waits.push(semaphore.raw());
        }
        hal_queue.add_signal_semaphore(release, None);
        Ok(StagedSubmissionSync {
            queue: self.queue.clone(),
            waits: waits.into_boxed_slice(),
            signal: release,
            consumed: false,
        })
    }
}

struct StagedSubmissionSync {
    queue: wgpu::Queue,
    waits: Box<[vk::Semaphore]>,
    signal: vk::Semaphore,
    consumed: bool,
}

impl StagedSubmissionSync {
    fn consume(mut self) {
        self.consumed = true;
    }
}

impl Drop for StagedSubmissionSync {
    fn drop(&mut self) {
        if self.consumed {
            return;
        }
        let Some(hal_queue) = (unsafe { self.queue.as_hal::<VkApi>() }) else {
            return;
        };
        for semaphore in &self.waits {
            hal_queue.remove_wait_semaphore(*semaphore);
        }
        hal_queue.remove_signal_semaphore(self.signal);
    }
}
