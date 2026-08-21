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
    wrappers::{Device, SemaphoreWaitValue, TimelineSemaphore},
};

struct SharedState {
    wait_requests: Mutex<Vec<SubmissionWaitRequest>>,
    waker_semaphore: WakerSemaphore,
    should_quit: AtomicBool,
}

pub(crate) struct WaiterThreadHandle {
    shared: Arc<SharedState>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl WaiterThreadHandle {
    pub(crate) fn submit(&self, request: SubmissionWaitRequest) -> Result<(), VulkanCommonError> {
        self.shared.wait_requests.lock().unwrap().push(request);
        self.shared.waker_semaphore.wake()
    }
}

impl Drop for WaiterThreadHandle {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };

        self.shared.should_quit.store(true, Ordering::Relaxed);
        self.shared.waker_semaphore.wake().unwrap();
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
            should_quit: AtomicBool::new(false),
            waker_semaphore: WakerSemaphore::new(device.clone())?,
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

                let quit = self.shared.should_quit.load(Ordering::Relaxed);
                if quit && requests.is_empty() {
                    return;
                }

                semaphores.push(self.shared.waker_semaphore.semaphore.semaphore);
                wait_values.push(self.shared.waker_semaphore.wait_value().0);
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

            // we can only read each semaphore's value once, because if we check more times and it changes between two checks,
            // we will run the finisher for a later frame before an earlier one
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

pub(crate) struct SubmissionTracker {
    waiter_thread: Arc<WaiterThreadHandle>,
    semaphore: Arc<TimelineSemaphore>,

    max_in_flight: usize,
    in_flight: VecDeque<Receiver<()>>,
}

impl SubmissionTracker {
    pub(crate) fn new(
        semaphore: Arc<TimelineSemaphore>,
        waiter_thread: Arc<WaiterThreadHandle>,
        max_in_flight: usize,
    ) -> Self {
        Self {
            waiter_thread,
            semaphore,
            max_in_flight,
            in_flight: VecDeque::new(),
        }
    }

    pub(crate) fn add_wait_request(
        &mut self,
        wait_for: SemaphoreWaitValue,
        timeout: Duration,
        on_finish: impl FnOnce() + Send + 'static,
    ) -> Result<(), VulkanCommonError> {
        let (finished_sender, finished_receiver) = std::sync::mpsc::channel();

        self.waiter_thread.submit(SubmissionWaitRequest {
            semaphore: self.semaphore.clone(),
            wait_for,
            on_finish: Box::new(move || {
                on_finish();
                let _ = finished_sender.send(());
            }),
        })?;

        if self.max_in_flight == 0 {
            // block until the wait request is done
            finished_receiver
                .recv_timeout(timeout)
                .map_err(|_| VulkanCommonError::SubmissionWaitTimeout)?;
        } else {
            self.in_flight.push_back(finished_receiver);
        }

        Ok(())
    }

    pub(crate) fn wait_if_full(&mut self, timeout: Duration) -> Result<(), VulkanCommonError> {
        if self.max_in_flight == 0 {
            return Ok(());
        }

        while self.in_flight.len() >= self.max_in_flight {
            self.in_flight
                .front()
                .unwrap()
                .recv_timeout(timeout)
                .map_err(|_| VulkanCommonError::SubmissionWaitTimeout)?;
            self.in_flight.pop_front();
        }

        Ok(())
    }

    pub(crate) fn wait_for_all(&mut self, timeout: Duration) -> Result<(), VulkanCommonError> {
        while let Some(receiver) = self.in_flight.front() {
            receiver
                .recv_timeout(timeout)
                .map_err(|_| VulkanCommonError::SubmissionWaitTimeout)?;
            self.in_flight.pop_front();
        }

        Ok(())
    }
}

struct WakerSemaphore {
    wait_for: Mutex<SemaphoreWaitValue>,
    semaphore: TimelineSemaphore,
}

impl WakerSemaphore {
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

    fn wake(&self) -> Result<(), VulkanCommonError> {
        // `wait_for` uses a mutex instead of atomic here by design.
        // If we used AtomicU64, 2 threads would fetch_add `wait_for` value
        // and then call signal with the `wait_for` value they fetched.
        // Depending on which thread signals first, this could result in
        // the semaphore being signaled in the wrong order (i.e. signaled with `2` and then `1`)
        let mut wait_for = self.wait_for.lock().unwrap();
        self.semaphore.signal(*wait_for)?;
        *wait_for = SemaphoreWaitValue(wait_for.0 + 1);
        Ok(())
    }
}
