use std::time::Duration;

use crossbeam_channel::{Receiver, bounded, tick};

/// Abstraction over the periodic tick that drives the queue thread.
/// Production uses [`RealTicker`] (wraps `crossbeam_channel::tick`); tests
/// supply their own implementation that only fires when explicitly pulsed.
pub(crate) trait Ticker: Send + Sync + std::fmt::Debug {
    fn receiver(&self) -> Receiver<()>;
}

#[derive(Debug)]
pub(crate) struct RealTicker {
    receiver: Receiver<std::time::Instant>,
}

impl RealTicker {
    pub fn new(interval: Duration) -> Self {
        Self {
            receiver: tick(interval),
        }
    }
}

impl Ticker for RealTicker {
    fn receiver(&self) -> Receiver<()> {
        // Adapt the Instant-emitting channel to a unit channel by mapping
        // through a relay thread. We could expose the original receiver
        // directly, but the queue thread only cares that *a* tick happened.
        let (tx, rx) = bounded::<()>(1);
        let inner = self.receiver.clone();
        std::thread::Builder::new()
            .name("Queue ticker relay".to_string())
            .spawn(move || {
                while inner.recv().is_ok() {
                    let _ = tx.try_send(());
                }
            })
            .unwrap();
        rx
    }
}

pub(crate) type SharedTicker = std::sync::Arc<dyn Ticker>;
