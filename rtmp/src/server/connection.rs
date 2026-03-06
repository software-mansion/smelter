use std::sync::Arc;

use crossbeam_channel::Receiver;

use crate::{RtmpEvent, utils::ShutdownCondition};

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
