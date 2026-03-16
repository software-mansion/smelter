use std::{sync::Arc, time::Duration};

use tracing::debug;

use crate::event::{Event, EventEmitter};

pub struct EmitEventOnce {
    event: Option<Event>,
    emitter: Arc<EventEmitter>,
}

impl EmitEventOnce {
    pub fn new(event: Event, emitter: &Arc<EventEmitter>) -> Self {
        Self {
            event: Some(event),
            emitter: emitter.clone(),
        }
    }

    pub fn emit(&mut self) {
        if let Some(event) = self.event.take() {
            debug!(?event, "Emitting event");
            self.emitter.emit(event)
        }
    }

    pub fn already_sent(&self) -> bool {
        self.event.is_none()
    }
}

pub struct PauseState {
    paused: bool,
    /// Internal PTS (relative to sync_point) when input was paused.
    paused_at_pts: Option<Duration>,
    /// Accumulated pause duration to add to frame/sample PTS after resume.
    pts_offset: Duration,
}

impl PauseState {
    pub fn new() -> Self {
        Self {
            paused: false,
            paused_at_pts: None,
            pts_offset: Duration::ZERO,
        }
    }

    pub fn pause(&mut self, pts: Duration) {
        // Make pause idempotent: if we're already paused, do not update the
        // original pause start timestamp to avoid distorting pts_offset.
        if self.paused {
            return;
        }
        self.paused = true;
        self.paused_at_pts = Some(pts);
    }

    pub fn resume(&mut self, pts: Duration, first_pts_received: bool) {
        self.paused = false;
        if let Some(pause_start) = self.paused_at_pts.take()
            && first_pts_received
        {
            self.pts_offset += pts.saturating_sub(pause_start);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn pts_offset(&self) -> Duration {
        self.pts_offset
    }
}
