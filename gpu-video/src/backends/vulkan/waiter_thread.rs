use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use ash::vk;
use rustc_hash::FxHashMap;

use crate::backends::vulkan::{
    VulkanCommonError,
    wrappers::{Device, SemaphoreWaitValue, TimelineSemaphore},
};

struct SharedState {
    wait_queues: Mutex<FxHashMap<vk::Semaphore, VecDeque<SubmissionWaitRequest>>>,
    interrupt_semaphore: InterruptSemaphore,
    quit_thread: AtomicBool,
}

pub(crate) struct WaiterThreadHandle {
    shared: Arc<SharedState>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl WaiterThreadHandle {
    pub(crate) fn spawn(device: Arc<Device>) -> Result<Self, VulkanCommonError> {
        let shared_state = Arc::new(SharedState {
            quit_thread: AtomicBool::new(false),
            wait_queues: Mutex::new(FxHashMap::default()),
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

        Ok(Self {
            shared: shared_state,
            handle: Some(handle),
        })
    }

    pub(crate) fn submit(
        &self,
        requests: Vec<SubmissionWaitRequest>,
    ) -> Result<(), VulkanCommonError> {
        let mut wait_queues = self.shared.wait_queues.lock().unwrap();
        for request in requests {
            wait_queues
                .entry(request.semaphore.semaphore)
                .or_default()
                .push_back(request);
        }

        self.shared.interrupt_semaphore.interrupt()
    }

    /// Waits for all submissions for that semaphore to finish
    pub(crate) fn wait_for_semaphore(
        &self,
        semaphore: Arc<TimelineSemaphore>,
    ) -> Result<(), VulkanCommonError> {
        let wait_for = semaphore.counter_value()?;
        let (sender, receiver) = std::sync::mpsc::channel();

        self.submit(vec![SubmissionWaitRequest {
            semaphore,
            wait_for,
            on_finish: Box::new(move || {
                let _ = sender.send(());
            }),
        }])?;

        receiver.recv().unwrap();
        Ok(())
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

struct WaiterThread {
    shared: Arc<SharedState>,
    device: Arc<Device>,
}

impl WaiterThread {
    fn run(self) {
        let mut semaphores = Vec::new();
        let mut wait_values = Vec::new();

        loop {
            let request_count = self.push_wait_semaphores(&mut semaphores, &mut wait_values);

            let quit = self.shared.quit_thread.load(Ordering::Relaxed);
            if quit && request_count == 0 {
                return;
            }

            let wait_info = vk::SemaphoreWaitInfo::default()
                .semaphores(&semaphores)
                .values(&wait_values)
                .flags(vk::SemaphoreWaitFlags::ANY);

            unsafe { self.device.wait_semaphores(&wait_info, u64::MAX).unwrap() };

            self.resolve_finished();
        }
    }

    fn push_wait_semaphores(
        &self,
        semaphores: &mut Vec<vk::Semaphore>,
        wait_values: &mut Vec<u64>,
    ) -> usize {
        semaphores.clear();
        wait_values.clear();

        semaphores.push(self.shared.interrupt_semaphore.semaphore.semaphore);
        wait_values.push(self.shared.interrupt_semaphore.wait_value().0);

        let wait_queues = self.shared.wait_queues.lock().unwrap();
        for (semaphore, queue) in wait_queues.iter() {
            let Some(head) = queue.front() else {
                continue;
            };
            semaphores.push(*semaphore);
            wait_values.push(head.wait_for.0);
        }

        wait_queues.values().map(|q| q.len()).sum()
    }

    fn resolve_finished(&self) {
        let mut finished = Vec::new();

        {
            let mut wait_queues = self.shared.wait_queues.lock().unwrap();
            for queue in wait_queues.values_mut() {
                while let Some(req) = queue.pop_front_if(|r| r.is_finished()) {
                    finished.push(req);
                }
            }
            wait_queues.retain(|_, q| !q.is_empty());
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

impl SubmissionWaitRequest {
    fn is_finished(&self) -> bool {
        self.semaphore.counter_value().unwrap() >= self.wait_for
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
