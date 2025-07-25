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

pub struct OffsetHandler {
    offset: Duration,
    first_pts: Option<Duration>,
}

impl OffsetHandler {
    pub fn new(offset: Duration) -> Self {
        Self {
            offset,
            first_pts: None,
        }
    }

    pub fn calculate_pts(&mut self, pts: Duration, queue_start_pts: Duration) -> Duration {
        let first_pts = *self.first_pts.get_or_insert(pts);
        queue_start_pts + self.offset + pts - first_pts
    }

    // Is current PTS before or after the offset
    // pts value was already transformed by `calculate_pts` function
    pub fn before_start(&self, pts: Duration, queue_start_pts: Duration) -> bool {
        self.offset + queue_start_pts > pts
    }
}
