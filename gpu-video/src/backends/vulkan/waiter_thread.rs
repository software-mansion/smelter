use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
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
