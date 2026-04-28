use std::process::Child;
use tracing::warn;

/// Wraps a spawned helper process. By default the child is **not** killed when
/// the handle is dropped — it is left running until the binary exits. This matches
/// the legacy "fire and forget" usage in examples. Call [`ProcessHandle::kill`] or
/// hold the handle inside a struct with a custom `Drop` impl when explicit shutdown
/// is desired (e.g. the interactive demo).
#[derive(Debug)]
pub struct ProcessHandle(Child);

impl ProcessHandle {
    pub(crate) fn new(child: Child) -> Self {
        Self(child)
    }

    /// Kill the child process. Ignores errors (logs a warning).
    pub fn kill(mut self) {
        if let Err(err) = self.0.kill() {
            warn!("Failed to kill child process: {err}");
        }
        let _ = self.0.wait();
    }

    /// Unwrap to the raw [`std::process::Child`].
    pub fn into_inner(self) -> Child {
        self.0
    }
}
