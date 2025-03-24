pub trait EventLoop {
    /// Runs the event loop. It must run on the main thread.
    fn run(&self) -> Result<(), EventLoopError>;

    fn run_single_loop(&self) -> Result<(), EventLoopError>;
}

#[derive(Debug, thiserror::Error)]
pub enum EventLoopError {
    #[error("Event loop must run on the main thread")]
    WrongThread,

    #[error("No event loop, no current features require main thread access.")]
    NoEventLoop,
}
