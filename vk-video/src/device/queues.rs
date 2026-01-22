use std::collections::HashSet;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ash::vk;

use crate::VulkanCommonError;
use crate::wrappers::*;

#[derive(Clone)]
pub(crate) struct Queue {
    pub(crate) queue: Arc<Mutex<vk::Queue>>,
    pub(crate) family_index: usize,
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

    pub(crate) fn submit_chain_semaphore<K: TrackerKind>(
        &self,
        buffer: RecordedCommandBuffer,
        tracker: &mut Tracker<K>,
        wait_stages: vk::PipelineStageFlags2,
        signal_stages: vk::PipelineStageFlags2,
        new_wait_state: K::WaitState,
    ) -> Result<SemaphoreWaitValue, VulkanCommonError> {
        let buffer_submit_info =
            [vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer())];

        let signal_value = tracker.semaphore_tracker.next_sem_value();
        let signal_info = vk::SemaphoreSubmitInfo::default()
            .semaphore(tracker.semaphore_tracker.semaphore.semaphore)
            .value(signal_value.0)
            .stage_mask(signal_stages);

        let wait_info = match tracker.semaphore_tracker.wait_for.take() {
            Some(wait_for) => Some(
                vk::SemaphoreSubmitInfo::default()
                    .semaphore(tracker.semaphore_tracker.semaphore.semaphore)
                    .value(wait_for.value.0)
                    .stage_mask(wait_stages),
            ),
            _ => None,
        };

        let mut submit_info = vk::SubmitInfo2::default()
            .signal_semaphore_infos(std::slice::from_ref(&signal_info))
            .command_buffer_infos(&buffer_submit_info);

        if let Some(wait_info) = wait_info.as_ref() {
            submit_info = submit_info.wait_semaphore_infos(std::slice::from_ref(wait_info));
        }

        unsafe {
            self.device.queue_submit2(
                *self.queue.lock().unwrap(),
                &[submit_info],
                vk::Fence::null(),
            )?
        };

        buffer.mark_submitted(&mut tracker.image_layout_tracker.lock().unwrap(), signal_value);

        tracker.semaphore_tracker.wait_for = Some(TrackerWait {
            value: signal_value,
            _state: new_wait_state,
        });

        Ok(signal_value)
    }
}

pub(crate) struct Queues {
    pub(crate) transfer: Queue,
    pub(crate) h264_decode: Option<Arc<VideoQueues>>,
    pub(crate) h264_encode: Option<Arc<VideoQueues>>,
    pub(crate) wgpu: Queue,
}

pub(crate) struct QueueIndex<'a> {
    pub(crate) family_index: usize,
    pub(crate) queue_count: usize,
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
    pub(crate) fn queue_create_infos(&self) -> Vec<QueueCreateInfo<'_>> {
        [
            self.h264_decode
                .as_ref()
                .map(|q| (q.family_index, q.queue_count)),
            self.h264_encode
                .as_ref()
                .map(|q| (q.family_index, q.queue_count)),
            Some((self.transfer.family_index, self.transfer.queue_count)),
            Some((
                self.graphics_transfer_compute.family_index,
                self.graphics_transfer_compute.queue_count,
            )),
        ]
        .into_iter()
        .flatten()
        .collect::<HashSet<(usize, usize)>>()
        .into_iter()
        .map(|(family_idx, queue_count)| QueueCreateInfo::new(family_idx, vec![1.0; queue_count]))
        .collect()
    }
}

pub(crate) struct QueueCreateInfo<'a> {
    pub(crate) info: vk::DeviceQueueCreateInfo<'a>,
    priorities_ptr: NonNull<[f32]>,
}

impl QueueCreateInfo<'_> {
    fn new(family_idx: usize, priorities: Vec<f32>) -> Self {
        let priorities_ref = Box::leak(priorities.into_boxed_slice());
        let priorities_ptr = NonNull::from(&mut *priorities_ref);
        let info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family_idx as u32)
            .queue_priorities(priorities_ref);

        Self {
            info,
            priorities_ptr,
        }
    }
}

impl Drop for QueueCreateInfo<'_> {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.priorities_ptr.as_ptr()) };
    }
}

pub(crate) struct VideoQueues {
    queues: Box<[Queue]>,
    current_queue_idx: AtomicUsize,
    pub(crate) family_index: usize,
}

impl VideoQueues {
    pub(crate) fn new(queues: Box<[Queue]>) -> Option<Self> {
        if queues.is_empty() {
            return None;
        }

        let family_index = queues[0].family_index;
        Some(Self {
            queues,
            current_queue_idx: AtomicUsize::new(0),
            family_index,
        })
    }

    fn next_queue(&self) -> &Queue {
        let idx = self.current_queue_idx.fetch_add(1, Ordering::Relaxed);
        &self.queues[idx % self.queues.len()]
    }

    pub(crate) fn supports_result_status_queries(&self) -> bool {
        // All queues from the same family share the same properties
        self.queues[0].supports_result_status_queries()
    }

    pub(crate) fn submit_chain_semaphore<K: TrackerKind>(
        &self,
        buffer: RecordedCommandBuffer,
        tracker: &mut Tracker<K>,
        wait_stages: vk::PipelineStageFlags2,
        signal_stages: vk::PipelineStageFlags2,
        new_wait_state: K::WaitState,
    ) -> Result<(), VulkanCommonError> {
        let queue = self.next_queue();
        queue.submit_chain_semaphore(buffer, tracker, wait_stages, signal_stages, new_wait_state)
    }
}
