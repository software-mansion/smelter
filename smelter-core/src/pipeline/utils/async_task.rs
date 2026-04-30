use std::sync::{Mutex, OnceLock};

use tokio::{runtime::Handle, task::JoinHandle};

pub(crate) struct AsyncTaskRegistry {
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl AsyncTaskRegistry {
    pub fn get() -> &'static Self {
        static REGISTRY: OnceLock<AsyncTaskRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| Self {
            handles: Mutex::new(Vec::new()),
        })
    }

    pub fn register(&self, handle: JoinHandle<()>) {
        self.handles.lock().unwrap().push(handle);
    }

    /// Abort every tracked task and synchronously wait for each to finish.
    ///
    /// Pipeline::drop may run inside the tokio runtime (e.g. when ApiState drops
    /// during axum::serve's future), so calling `Handle::block_on` directly here
    /// would panic ("Cannot start a runtime from within a runtime"). To stay safe
    /// regardless of caller context, run the wait on a dedicated OS thread.
    pub fn abort_and_join(&self, rt: &Handle) {
        let handles: Vec<_> = std::mem::take(&mut *self.handles.lock().unwrap());
        if handles.is_empty() {
            return;
        }
        for h in &handles {
            h.abort();
        }
        let rt = rt.clone();
        let join_thread = std::thread::Builder::new()
            .name("AsyncTaskRegistry join".to_string())
            .spawn(move || {
                rt.block_on(async {
                    for h in handles {
                        let _ = h.await;
                    }
                });
            })
            .expect("Failed to spawn AsyncTaskRegistry join thread");
        let _ = join_thread.join();
    }
}
