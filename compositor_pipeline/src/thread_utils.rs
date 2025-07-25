use tracing::{span, Level, Span};

pub(crate) trait InitializableThread: Sized {
    type InitOptions: Send + 'static;

    /// Represents type returned on successful `init` to the caller of `spawn_thread`
    type SpawnOutput: Send + 'static;
    /// Represents type returned on failed `init` to the caller of `spawn_thread`
    type SpawnError: std::error::Error + Send + 'static;

    /// Internal thread state passed to `run`
    type ThreadState;

    const LABEL: &'static str;

    fn init(
        options: Self::InitOptions,
    ) -> Result<(Self::SpawnOutput, Self::ThreadState), Self::SpawnError>;

    fn thread_span(instance_id: &str) -> Span {
        span!(Level::INFO, "Thread", label = Self::LABEL, instance_id)
    }

    fn run(state: Self::ThreadState);
}

pub(crate) fn spawn_thread<Thread: InitializableThread>(
    thread_instance_id: &str,
    opts: Thread::InitOptions,
) -> Result<Thread::SpawnOutput, Thread::SpawnError> {
    let (result_sender, result_receiver) = crossbeam_channel::bounded(0);

    let thread_span = Thread::thread_span(thread_instance_id);
    std::thread::Builder::new()
        .name(format!("Thread {}: {}", Thread::LABEL, thread_instance_id))
        .spawn(move || {
            let _span = thread_span.entered();
            let state = match Thread::init(opts) {
                Ok((result, state)) => {
                    result_sender.send(Ok(result)).unwrap();
                    state
                }
                Err(err) => {
                    result_sender.send(Err(err)).unwrap();
                    return;
                }
            };
            Thread::run(state);
        })
        .unwrap();

    result_receiver.recv().unwrap()
}
