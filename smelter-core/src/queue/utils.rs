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
        if self.sent {
            return;
        }
        self.sent = true;
        self.emitter.emit(self.event.clone());
    }

    pub fn reset(&mut self) {
        self.sent = false;
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum QueueState {
    New,
    Running,
    /// After EOS was received, do not attempt to enqueue
    /// packets, after queue is drained go to `Restarted`
    Draining,
    /// Should behave the same as New, but it always returns
    /// true when enqueuing until ready. Required input at the start
    /// should wait for first frame even if it is scheduled for
    /// the future, but this should not happen after restart.
    Restarted,
}

pub struct PauseState {
    /// Internal PTS (relative to sync_point) when input was paused.
    paused_at_pts: Option<Duration>,
    /// Accumulated pause duration to add to frame/sample PTS after resume.
    pts_offset: Duration,
}

impl PauseState {
    pub fn new() -> Self {
        Self {
            paused_at_pts: None,
            pts_offset: Duration::ZERO,
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
    pub fn resume(&mut self, pts: Duration, state: QueueState) -> bool {
        let Some(pause_start) = self.paused_at_pts.take() else {
            return false;
        };
        match state {
            QueueState::New | QueueState::Restarted => {
                // Input without offset then pause offset should be increased based on first
                // incoming packet
                // Input with offset no change
                //
                // Because we care here mostly for MP4 for now, then this case can only
                // happen for quick pause after registering.
                self.pts_offset += pts.saturating_sub(pause_start);
            }
            QueueState::Running => {
                self.pts_offset += pts.saturating_sub(pause_start);
            }
            QueueState::Draining => {
                // We will clear the queue, send EOS on the next loop
                // and state will be reset, so offset does not matter
                // after this
            }
        }
        true
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at_pts().is_some()
    }

    pub fn pts_offset(&self) -> Duration {
        self.pts_offset
    }

    pub fn paused_at_pts(&self) -> Option<Duration> {
        self.paused_at_pts
    }

    pub fn reset(&mut self) {
        self.pts_offset = Duration::ZERO;
    }
}
