use std::sync::Arc;
use std::time::Duration;

use crate::event::{Event, EventEmitter};

/// Guards against emitting a particular event more than once.
/// Use `reset` to re-arm after EOS so the event can fire again.
pub struct EmitOnceGuard {
    sent: bool,
    event: Event,
    emitter: Arc<EventEmitter>,
}

impl EmitOnceGuard {
    pub fn new(event: Event, emitter: &Arc<EventEmitter>) -> Self {
        Self {
            sent: false,
            event,
            emitter: emitter.clone(),
        }
    }

    /// Emits the event if it hasn't been sent yet.
    pub fn emit(&mut self) {
        if !self.sent {
            self.emitter.emit(self.event.clone());
            self.sent = true;
        }
    }

    pub fn emited(&self) -> bool {
        self.sent
    }

    pub fn reset(&mut self) {
        self.sent = false
    }
}

pub struct PauseState {
    /// Internal PTS (relative to sync_point) when input was paused.
    paused_at_pts: Option<Duration>,
}

impl PauseState {
    pub fn new() -> Self {
        Self {
            paused_at_pts: None,
        }
    }

    /// Sets paused state. Returns `true` if the pause state was changed
    pub fn pause(&mut self, pts: Duration) -> bool {
        if self.paused_at_pts.is_some() {
            return false;
        }
        self.paused_at_pts = Some(pts);
        true
    }

    /// Clears paused state. Returns `true` if pause state was changed
    pub fn resume(&mut self, pts: Duration) -> Option<Duration> {
        let pause_start = self.paused_at_pts.take()?;
        Some(pts.saturating_sub(pause_start))
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at_pts().is_some()
    }

    pub fn paused_at_pts(&self) -> Option<Duration> {
        self.paused_at_pts
    }

    pub fn reset(&mut self, pts: Duration) {
        self.paused_at_pts = Some(pts);
    }
}
