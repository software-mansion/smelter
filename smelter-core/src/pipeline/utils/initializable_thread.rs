use std::thread::JoinHandle;

use tracing::{Level, span};

/// Owns a `JoinHandle<()>` and joins it on drop. Store this *after* any
/// `Sender`s that feed the worker thread, so the senders drop first (closing
/// the channel and signaling the worker to exit) before the joiner waits.
///
/// The reason this exists at all is the NVIDIA libvulkan atexit race: any
/// worker thread that holds an `Arc<vk_video::*>` clone must finish before the
/// process exits, otherwise `vkDestroy*` may run on the worker thread
/// concurrently with NVIDIA's atexit handlers.
#[derive(Debug)]
pub(crate) struct ThreadJoiner {
    handle: Option<JoinHandle<()>>,
}

impl ThreadJoiner {
    pub(crate) fn new(handle: JoinHandle<()>) -> Self {
        Self {
            handle: Some(handle),
        }
    }
}

impl Drop for ThreadJoiner {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take()
            && let Err(err) = handle.join()
        {
            tracing::error!(?err, "Worker thread panicked during join");
        }
    }
}

pub(crate) trait InitializableThread: Sized {
    type InitOptions: Send + 'static;

    /// Represents type returned on successful `init` to the caller of `Self::spawn`
    type SpawnOutput: Send + 'static;
    /// Represents type returned on failed `init` to the caller of `Self::spawn`
    type SpawnError: std::error::Error + Send + 'static;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError>;

    fn run(self);

    /// Spawn the thread and wait for `init` to complete. On success, the caller
    /// receives both the init output and the `JoinHandle` for the spawned
    /// thread. The handle MUST be retained and joined during shutdown — leaking
    /// it (drop without join) lets the thread outlive `Pipeline` and risks a
    /// race with the NVIDIA libvulkan atexit handler if the thread holds any
    /// `Arc<vk_video::*>` clone.
    fn spawn<Id: ToString>(
        thread_instance_id: Id,
        opts: Self::InitOptions,
    ) -> Result<(Self::SpawnOutput, JoinHandle<()>), Self::SpawnError> {
        let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

        let instance_id = thread_instance_id.to_string();
        let metadata = Self::metadata();
        let handle = std::thread::Builder::new()
            .name(metadata.thread_name.to_string())
            .spawn(move || {
                let _span = span!(
                    Level::INFO,
                    "Thread",
                    thread = metadata.thread_name,
                    instance = format!("{} {}", metadata.thread_instance_name, instance_id),
                )
                .entered();
                let state = match Self::init(opts) {
                    Ok((state, init_output)) => {
                        result_sender.send(Ok(init_output)).unwrap();
                        state
                    }
                    Err(err) => {
                        result_sender.send(Err(err)).unwrap();
                        return;
                    }
                };
                Self::run(state);
            })
            .unwrap();

        match result_receiver.recv().unwrap() {
            Ok(output) => Ok((output, handle)),
            Err(err) => {
                // The thread already returned (init failed). Join it so it
                // doesn't outlive this call.
                let _ = handle.join();
                Err(err)
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: "Initializable thread".to_string(),
            thread_instance_name: "Instance".to_string(),
        }
    }
}

pub(crate) struct ThreadMetadata {
    pub thread_name: String,
    pub thread_instance_name: String,
}
