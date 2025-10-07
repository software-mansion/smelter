use std::sync::{Arc, Mutex};

use ash::vk;

use crate::VulkanCommonError;
use crate::wrappers::*;

#[derive(Clone)]
pub(crate) struct Queue {
    pub(crate) queue: Arc<Mutex<vk::Queue>>,
    pub(crate) idx: usize,
    pub(crate) _video_properties: vk::QueueFamilyVideoPropertiesKHR<'static>,
    pub(crate) query_result_status_properties:
        vk::QueueFamilyQueryResultStatusPropertiesKHR<'static>,
    pub(crate) device: Arc<Device>,
}

impl Queue {
    pub(crate) fn supports_result_status_queries(&self) -> bool {
        self.query_result_status_properties
            .query_result_status_support
            == vk::TRUE
    }

    pub(crate) fn submit(
        &self,
        buffer: &CommandBuffer,
        wait_semaphores: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        signal_semaphores: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        fence: Option<vk::Fence>,
    ) -> Result<(), VulkanCommonError> {
        fn to_sem_submit_info(
            submits: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        ) -> Vec<vk::SemaphoreSubmitInfo<'_>> {
            submits
                .iter()
                .map(|&(sem, stage)| {
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(sem)
                        .stage_mask(stage)
                })
                .collect::<Vec<_>>()
        }

        let wait_semaphores = to_sem_submit_info(wait_semaphores);
        let signal_semaphores = to_sem_submit_info(signal_semaphores);

        let buffer_submit_info =
            [vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer)];

        let submit_info = [vk::SubmitInfo2::default()
            .wait_semaphore_infos(&wait_semaphores)
            .signal_semaphore_infos(&signal_semaphores)
            .command_buffer_infos(&buffer_submit_info)];

        unsafe {
            self.device.queue_submit2(
                *self.queue.lock().unwrap(),
                &submit_info,
                fence.unwrap_or(vk::Fence::null()),
            )?
        };

        Ok(())
    }
}

pub(crate) struct Queues {
    pub(crate) transfer: Queue,
    pub(crate) h264_decode: Option<Queue>,
    pub(crate) h264_encode: Option<Queue>,
    pub(crate) wgpu: Queue,
}

pub(crate) struct QueueIndex<'a> {
    pub(crate) idx: usize,
    pub(crate) video_properties: vk::QueueFamilyVideoPropertiesKHR<'a>,
    pub(crate) query_result_status_properties: vk::QueueFamilyQueryResultStatusPropertiesKHR<'a>,
}

pub(crate) struct QueueIndices<'a> {
    pub(crate) transfer: QueueIndex<'a>,
    pub(crate) h264_decode: Option<QueueIndex<'a>>,
    pub(crate) h264_encode: Option<QueueIndex<'a>>,
    pub(crate) graphics_transfer_compute: QueueIndex<'a>,
}

impl QueueIndices<'_> {
    pub(crate) fn queue_create_infos(&self) -> Vec<vk::DeviceQueueCreateInfo<'_>> {
        [
            self.h264_decode.as_ref().map(|q| q.idx),
            self.h264_encode.as_ref().map(|q| q.idx),
            Some(self.transfer.idx),
            Some(self.graphics_transfer_compute.idx),
        ]
        .into_iter()
        .flatten()
        .collect::<std::collections::HashSet<usize>>()
        .into_iter()
        .map(|i| {
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(i as u32)
                .queue_priorities(&[1.0])
        })
        .collect::<Vec<_>>()
    }
}
