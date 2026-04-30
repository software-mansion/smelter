use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle, ThreadId};
use tracing::error;

type Joiner = Box<dyn FnOnce() + Send>;

struct Entry {
    id: ThreadId,
    name: String,
    joiner: Joiner,
}

pub struct ThreadRegistry {
    joiners: Mutex<Vec<Entry>>,
}

impl ThreadRegistry {
    pub fn get() -> &'static Self {
        static REGISTRY: OnceLock<ThreadRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| Self {
            joiners: Mutex::new(Vec::new()),
        })
    }

    pub fn spawn<F, T>(&self, name: String, f: F)
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let handle = thread::Builder::new()
            .name(name)
            .spawn(f)
            .expect("Failed to spawn thread");
        self.register(handle);
    }

    pub fn register<T: Send + 'static>(&self, handle: JoinHandle<T>) {
        let id = handle.thread().id();
        let name = handle.thread().name().unwrap_or("unnamed").to_string();
        let name_for_closure = name.clone();
        let joiner: Joiner = Box::new(move || {
            if let Err(e) = handle.join() {
                error!("Thread \"{}\" panicked: {:?}", name_for_closure, e);
            }
        });
        self.joiners
            .lock()
            .unwrap()
            .push(Entry { id, name, joiner });
    }

    /// Join all registered threads. If `Pipeline::drop` happens to run on a registered
    /// thread (e.g. the renderer thread won a race for the last strong `Arc<Mutex<Pipeline>>`),
    /// we'd self-deadlock by trying to join ourselves. Detach the current thread's joiner
    /// instead — its work is winding down anyway and the OS will reap it on exit.
    pub fn join_all(&self) {
        let current = thread::current().id();
        let drained: Vec<Entry> = std::mem::take(&mut *self.joiners.lock().unwrap());
        for entry in drained {
            if entry.id == current {
                error!("ThreadRegistry: detaching self \"{}\"", entry.name);
                drop(entry.joiner);
                continue;
            }
            error!("ThreadRegistry: joining \"{}\"", entry.name);
            (entry.joiner)();
            error!("ThreadRegistry: joined \"{}\"", entry.name);
        }
    }
}
