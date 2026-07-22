use std::{sync::Arc, time::Duration};

use crossbeam_channel::{Receiver, RecvTimeoutError};

use crate::{RtmpEvent, utils::ShutdownCondition};

#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtmpRecvTimeoutError {
    #[error("Timed out while waiting for the next event")]
    Timeout,

    #[error("Connection closed")]
    ConnectionClosed,
}

pub struct RtmpServerConnection {
    pub(super) app: Arc<str>,
    pub(super) stream_key: Arc<str>,
    pub(super) receiver: Receiver<RtmpEvent>,
    pub(super) shutdown_condition: ShutdownCondition,
}

impl RtmpServerConnection {
    pub fn app(&self) -> &Arc<str> {
        &self.app
    }

    pub fn stream_key(&self) -> &Arc<str> {
        &self.stream_key
    }

    /// Wait for the next event, but no longer than `timeout`.
    pub fn next_event_timeout(&self, timeout: Duration) -> Result<RtmpEvent, RtmpRecvTimeoutError> {
        self.receiver.recv_timeout(timeout).map_err(|err| match err {
            RecvTimeoutError::Timeout => RtmpRecvTimeoutError::Timeout,
            RecvTimeoutError::Disconnected => RtmpRecvTimeoutError::ConnectionClosed,
        })
    }

    /// Force close the connection. Calling this function is not required
    /// for cleanup. it is useful when you can't drop the connection because
    /// you are blocked in iterator loop.
    pub fn close(&self) {
        self.shutdown_condition.mark_for_shutdown();
    }
}

impl Drop for RtmpServerConnection {
    fn drop(&mut self) {
        self.shutdown_condition.mark_for_shutdown();
    }
}

impl Iterator for &RtmpServerConnection {
    type Item = RtmpEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}
