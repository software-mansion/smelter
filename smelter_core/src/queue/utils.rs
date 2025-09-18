use std::sync::Arc;

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
