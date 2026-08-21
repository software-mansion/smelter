use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
    time::Duration,
};

use ash::vk;
use rustc_hash::FxHashMap;
use tracing::debug;

use crate::backends::vulkan::{
    VulkanCommonError,
    wrappers::{CommandBufferPoolStorage, Device, SemaphoreWaitValue, TimelineSemaphore},
};

struct SharedState {
    wait_requests: Mutex<Vec<SubmissionWaitRequest>>,
    interrupt_semaphore: InterruptSemaphore,
    quit_thread: AtomicBool,
}

pub(crate) struct WaiterThreadHandle {
    shared: Arc<SharedState>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl WaiterThreadHandle {
    pub(crate) fn submit(&self, request: SubmissionWaitRequest) -> Result<(), VulkanCommonError> {
        self.shared.wait_requests.lock().unwrap().push(request);
        self.shared.interrupt_semaphore.interrupt()
    }
}

impl Drop for WaiterThreadHandle {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };

        self.shared.quit_thread.store(true, Ordering::Relaxed);
        self.shared.interrupt_semaphore.interrupt().unwrap();
        handle.join().unwrap()
    }
}

pub(crate) struct WaiterThread {
    shared: Arc<SharedState>,
    device: Arc<Device>,
}

impl WaiterThread {
    pub(crate) fn spawn(device: Arc<Device>) -> Result<WaiterThreadHandle, VulkanCommonError> {
        let shared_state = Arc::new(SharedState {
            wait_requests: Mutex::new(Vec::new()),
            quit_thread: AtomicBool::new(false),
            interrupt_semaphore: InterruptSemaphore::new(device.clone())?,
        });

        let shared_state_clone = shared_state.clone();
        let handle = std::thread::Builder::new()
            .name("gpu-video: submission waiter thread".to_string())
            .spawn(move || {
                WaiterThread {
                    shared: shared_state_clone,
                    device,
                }
                .run();
            })
            .unwrap();

        Ok(WaiterThreadHandle {
            shared: shared_state,
            handle: Some(handle),
        })
    }

    fn run(self) {
        let mut semaphores = Vec::new();
        let mut wait_values = Vec::new();

        loop {
            semaphores.clear();
            wait_values.clear();

            {
                let requests = self.shared.wait_requests.lock().unwrap();

                let quit = self.shared.quit_thread.load(Ordering::Relaxed);
                if quit && requests.is_empty() {
                    return;
                }

                semaphores.push(self.shared.interrupt_semaphore.semaphore.semaphore);
                wait_values.push(self.shared.interrupt_semaphore.wait_value().0);
                for req in requests.iter() {
                    semaphores.push(req.semaphore.semaphore);
                    wait_values.push(req.wait_for.0);
                }
            }

            let wait_info = vk::SemaphoreWaitInfo::default()
                .semaphores(&semaphores)
                .values(&wait_values)
                .flags(vk::SemaphoreWaitFlags::ANY);

            if let Err(err) = unsafe { self.device.wait_semaphores(&wait_info, u64::MAX) } {
                debug!("Failed to wait for semaphores {err}");
            }

            self.resolve_finished();
        }
    }

    fn resolve_finished(&self) {
        let finished = {
            let mut requests = self.shared.wait_requests.lock().unwrap();

            // semaphore values can increase as we iterate, so it's possible that the later submissions would qualify as finished while the early ones wouldn't
            let mut semaphore_values = FxHashMap::default();
            let (finished, unfinished) =
                std::mem::take(&mut *requests).into_iter().partition(|r| {
                    let value = semaphore_values
                        .entry(r.semaphore.semaphore)
                        .or_insert(r.semaphore.counter_value().unwrap());
                    *value >= r.wait_for
                });
            *requests = unfinished;
            finished
        };

        for req in finished {
            (req.on_finish)();
        }
    }
}

pub(crate) struct SubmissionWaitRequest {
    pub(crate) semaphore: Arc<TimelineSemaphore>,
    pub(crate) wait_for: SemaphoreWaitValue,
    pub(crate) on_finish: Box<dyn FnOnce() + Send>,
}

pub(crate) struct SubmissionTracker<P: CommandBufferPoolStorage> {
    waiter_thread: Arc<WaiterThreadHandle>,
    semaphore: Arc<TimelineSemaphore>,
    command_buffer_pools: P,

    max_in_flight: usize,
    in_flight: VecDeque<Receiver<()>>,

    submission_failed: Arc<AtomicBool>,
}

impl<P: CommandBufferPoolStorage + Clone + Send + 'static> SubmissionTracker<P> {
    pub(crate) fn new(
        command_buffer_pools: P,
        semaphore: Arc<TimelineSemaphore>,
        waiter_thread: Arc<WaiterThreadHandle>,
        max_in_flight: usize,
        submission_failed: Arc<AtomicBool>,
    ) -> Self {
        Self {
            waiter_thread,
            semaphore,
            command_buffer_pools,
            max_in_flight,
            in_flight: VecDeque::new(),
            submission_failed,
        }
    }

    pub(crate) fn add_wait_request<E: std::fmt::Display>(
        &mut self,
        wait_for: SemaphoreWaitValue,
        on_finish: impl FnOnce() -> Result<(), E> + Send + 'static,
    ) -> Result<(), VulkanCommonError> {
        let command_buffer_pools = self.command_buffer_pools.clone();
        let submission_failed = self.submission_failed.clone();
        let (finished_sender, finished_receiver) = std::sync::mpsc::channel();

        self.waiter_thread.submit(SubmissionWaitRequest {
            semaphore: self.semaphore.clone(),
            wait_for,
            on_finish: Box::new(move || {
                command_buffer_pools.mark_submitted_as_free(wait_for);
                if let Err(err) = on_finish() {
                    debug!("Submission processing failed: {err}");
                    submission_failed.store(true, Ordering::Relaxed);
                }
                let _ = finished_sender.send(());
            }),
        })?;

        self.in_flight.push_back(finished_receiver);
        Ok(())
    }

    pub(crate) fn wait_if_full(&mut self) -> Result<(), VulkanCommonError> {
        while self.in_flight.len() >= self.max_in_flight {
            self.in_flight
                .front()
                .unwrap()
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| VulkanCommonError::SubmissionWaitTimeout)?;
            self.in_flight.pop_front();
        }

        Ok(())
    }

    pub(crate) fn wait_for_all(&mut self) -> Result<(), VulkanCommonError> {
        while let Some(receiver) = self.in_flight.front() {
            receiver
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| VulkanCommonError::SubmissionWaitTimeout)?;
            self.in_flight.pop_front();
        }

        Ok(())
    }
}

struct InterruptSemaphore {
    wait_for: Mutex<SemaphoreWaitValue>,
    semaphore: TimelineSemaphore,
}

impl InterruptSemaphore {
    fn new(device: Arc<Device>) -> Result<Self, VulkanCommonError> {
        let wait_for = Mutex::new(SemaphoreWaitValue(1));
        let semaphore = TimelineSemaphore::new(device, 0, Some("interrupt submission wait"))?;

        Ok(Self {
            wait_for,
            semaphore,
        })
    }

    fn wait_value(&self) -> SemaphoreWaitValue {
        *self.wait_for.lock().unwrap()
    }

    fn interrupt(&self) -> Result<(), VulkanCommonError> {
        let mut wait_for = self.wait_for.lock().unwrap();
        self.semaphore.signal(*wait_for)?;
        *wait_for = SemaphoreWaitValue(wait_for.0 + 1);
        Ok(())
    }
}
