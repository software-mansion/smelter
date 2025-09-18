use std::{
    collections::HashSet,
    mem,
    sync::{OnceLock, RwLock},
};

use crossbeam_channel::{unbounded, Receiver, Sender};
use log::trace;

#[derive(Debug, Clone)]
pub struct Event {
    pub kind: String,
    pub properties: Vec<(String, String)>,
}

pub fn emit_event<T: Into<Event>>(event: T) {
    global_instance().send_event(event)
}

pub fn subscribe() -> Receiver<Event> {
    global_instance().subscribe()
}

pub struct Emitter<E: Clone> {
    subscribers: RwLock<Vec<Sender<E>>>,
}

fn global_instance() -> &'static Emitter<Event> {
    static EMITTER: OnceLock<Emitter<Event>> = OnceLock::new();
    EMITTER.get_or_init(Emitter::new)
}

impl<E: Clone> Emitter<E> {
    pub fn new() -> Self {
        Self {
            subscribers: vec![].into(),
        }
    }

    pub fn send_event<T: Into<E>>(&self, event: T) {
        let event = event.into();
        let mut disconnected_subscriber_indexes = HashSet::new();
        for (index, subscriber) in self.subscribers.read().unwrap().iter().enumerate() {
            if let Err(_err) = subscriber.send(event.clone()) {
                trace!("Event subscriber disconnected.");
                disconnected_subscriber_indexes.insert(index);
            }
        }
        if !disconnected_subscriber_indexes.is_empty() {
            self.remove_disconnected_subscribers(disconnected_subscriber_indexes)
        }
    }

    pub fn subscribe(&self) -> Receiver<E> {
        let (sender, receiver) = unbounded();
        self.subscribers.write().unwrap().push(sender);
        receiver
    }

    fn remove_disconnected_subscribers(&self, disconnected: HashSet<usize>) {
        let mut guard = self.subscribers.write().unwrap();
        *guard = mem::take(&mut *guard)
            .into_iter()
            .enumerate()
            .filter_map(|(index, sender)| {
                if disconnected.contains(&index) {
                    None
                } else {
                    Some(sender)
                }
            })
            .collect()
    }
}

impl<E: Clone> Default for Emitter<E> {
    fn default() -> Self {
        Self::new()
    }
}
