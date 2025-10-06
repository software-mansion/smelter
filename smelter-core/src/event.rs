use std::fmt::Debug;

use crossbeam_channel::Receiver;
use smelter_render::{
    InputId, OutputId,
    event_handler::{self, Emitter, emit_event},
};

#[derive(Debug, Clone)]
pub enum Event {
    AudioInputStreamDelivered(InputId),
    VideoInputStreamDelivered(InputId),
    AudioInputStreamPlaying(InputId),
    VideoInputStreamPlaying(InputId),
    AudioInputStreamEos(InputId),
    VideoInputStreamEos(InputId),
    OutputDone(OutputId),
}

fn input_event(kind: &str, input_id: InputId) -> event_handler::Event {
    event_handler::Event {
        kind: kind.to_string(),
        properties: vec![("input_id".to_string(), input_id.to_string())],
    }
}

fn output_event(kind: &str, output_id: OutputId) -> event_handler::Event {
    event_handler::Event {
        kind: kind.to_string(),
        properties: vec![("output_id".to_string(), output_id.to_string())],
    }
}

impl From<Event> for event_handler::Event {
    fn from(val: Event) -> Self {
        match val {
            Event::AudioInputStreamDelivered(id) => input_event("AUDIO_INPUT_DELIVERED", id),
            Event::VideoInputStreamDelivered(id) => input_event("VIDEO_INPUT_DELIVERED", id),
            Event::AudioInputStreamPlaying(id) => input_event("AUDIO_INPUT_PLAYING", id),
            Event::VideoInputStreamPlaying(id) => input_event("VIDEO_INPUT_PLAYING", id),
            Event::AudioInputStreamEos(id) => input_event("AUDIO_INPUT_EOS", id),
            Event::VideoInputStreamEos(id) => input_event("VIDEO_INPUT_EOS", id),
            Event::OutputDone(id) => output_event("OUTPUT_DONE", id),
        }
    }
}

pub struct EventEmitter {
    emitter: Emitter<Event>,
}

impl Debug for EventEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventEmitter").finish()
    }
}

impl EventEmitter {
    pub(super) fn new() -> Self {
        Self {
            emitter: Emitter::new(),
        }
    }

    pub(super) fn emit(&self, event: Event) {
        self.emitter.send_event(event.clone());
        // emit global event
        emit_event(event)
    }

    pub(super) fn subscribe(&self) -> Receiver<Event> {
        self.emitter.subscribe()
    }
}
