use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ash::vk;

use crate::VulkanCommonError;
use crate::vulkan_decoder::DecoderId;
use crate::vulkan_encoder::EncoderId;
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
    ) -> Result<(), VulkanCommonError> {
        let buffer_submit_info =
            [vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer())];

        let signal_value = tracker.semaphore_tracker.next_sem_value();
        let signal_info = vk::SemaphoreSubmitInfo::default()
            .semaphore(tracker.semaphore_tracker.semaphore.semaphore)
            .value(signal_value)
            .stage_mask(signal_stages);

        let wait_info = match tracker.semaphore_tracker.wait_for.take() {
            Some(wait_for) => Some(
                vk::SemaphoreSubmitInfo::default()
                    .semaphore(tracker.semaphore_tracker.semaphore.semaphore)
                    .value(wait_for.value)
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

        buffer.mark_submitted(&mut tracker.image_layout_tracker.lock().unwrap());

        tracker.semaphore_tracker.wait_for = Some(TrackerWait {
            value: signal_value,
            _state: new_wait_state,
        });

        Ok(())
    }
}

pub(crate) struct Queues {
    pub(crate) transfer: Queue,
    pub(crate) h264_decode: Vec<VideoQueue<DecoderId>>,
    pub(crate) h264_encode: Vec<VideoQueue<EncoderId>>,
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

#[derive(Clone)]
pub(crate) struct VideoQueue<Id> {
    pub(crate) queue: Queue,
    stats: Arc<Mutex<HashMap<Id, VideoQueueStats>>>,
}

impl<Id> Deref for VideoQueue<Id> {
    type Target = Queue;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

impl<Id: Hash + Eq> VideoQueue<Id> {
    pub(crate) fn new(queue: Queue) -> Self {
        Self {
            queue,
            stats: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn log_resolution_change(&self, id: Id, resolution: vk::Extent2D) {
        let mut stats = self.stats.lock().unwrap();
        let specific_stats = stats.entry(id).or_default();
        specific_stats.pixels_per_frame = resolution.width as usize * resolution.height as usize;
    }

    pub(crate) fn log_video_operation(&self, id: Id) {
        const MAX_TIMESTAMP_BUFFER: usize = 10;

        let mut stats = self.stats.lock().unwrap();
        let specific_stats = stats.entry(id).or_default();
        let timestamps = &mut specific_stats.video_operation_timestamps;
        timestamps.push_back(Instant::now());

        while timestamps.len() > MAX_TIMESTAMP_BUFFER {
            timestamps.pop_front();
        }
    }

    pub(crate) fn remove_stats(&self, id: &Id) {
        let mut stats = self.stats.lock().unwrap();
        let _ = stats.remove(id);
    }

    /// Calculates workload based on usage statistics.
    /// Higher number means more work is done by the queue
    pub(crate) fn calculate_workload(&self) -> f32 {
        let mut stats = self.stats.lock().unwrap();
        stats.values_mut().map(|s| s.calculate_workload()).sum()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VideoQueueStats {
    pub(crate) pixels_per_frame: usize,
    /// Contains timestamps of the queue doing video operations
    pub(crate) video_operation_timestamps: VecDeque<Instant>,
}

impl Default for VideoQueueStats {
    fn default() -> Self {
        Self {
            pixels_per_frame: 1,
            video_operation_timestamps: Default::default(),
        }
    }
}

impl VideoQueueStats {
    fn calculate_workload(&mut self) -> f32 {
        self.drop_old_timestamps();
        let framerate = self.calculate_average_framerate();
        self.pixels_per_frame as f32 * framerate
    }

    fn calculate_average_framerate(&self) -> f32 {
        let timestamps = &self.video_operation_timestamps;
        if timestamps.len() < 2 {
            return 1.0;
        }

        let total_elapsed: Duration = (0..timestamps.len() - 1)
            .map(|i| timestamps[i + 1].duration_since(timestamps[i]))
            .sum();
        (timestamps.len() - 1) as f32 / total_elapsed.as_secs_f32()
    }

    fn drop_old_timestamps(&mut self) {
        const OLDEST_TIMESTAMP_ELAPSED: Duration = Duration::from_secs(2);
        self.video_operation_timestamps
            .retain(|timestamp| timestamp.elapsed() < OLDEST_TIMESTAMP_ELAPSED);
    }
}

/// Finds queue which has the least amount of workload.
/// If there are multiple queues with the same low workload, the queue will be chosen at random.
pub(crate) fn find_available_video_queue<Id: Hash + Eq + Clone>(
    queues: &[VideoQueue<Id>],
) -> Option<VideoQueue<Id>> {
    let mut queues = queues
        .iter()
        .map(|q| (q, q.calculate_workload()))
        .collect::<Vec<_>>();
    let lowest_workload = queues
        .iter()
        .min_by(|(_, workload_1), (_, workload_2)| workload_1.total_cmp(workload_2))
        .map(|(_, workload)| *workload)?;

    queues.retain(|(_, workload)| (*workload - lowest_workload).abs() < 1e-7);

    let (queue, _) = queues[rand::random_range(0..queues.len())];
    Some(queue.clone())
}
