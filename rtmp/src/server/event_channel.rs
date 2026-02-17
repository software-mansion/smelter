use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

use crate::RtmpEvent;

#[derive(Debug, Clone)]
pub struct RtmpEventBufferSnapshot {
    pub len: usize,
    pub first: Option<RtmpEvent>,
    pub last: Option<RtmpEvent>,
}

#[derive(Debug)]
pub struct RtmpEventSendError {
    pub event: RtmpEvent,
}

#[derive(Debug)]
pub struct RtmpEventSender {
    shared: Arc<(Mutex<ChannelState>, Condvar)>,
}

#[derive(Debug)]
pub struct RtmpEventReceiver {
    shared: Arc<(Mutex<ChannelState>, Condvar)>,
}

#[derive(Debug, Default)]
struct ChannelState {
    queue: VecDeque<RtmpEvent>,
    sender_count: usize,
    receiver_alive: bool,
}

pub fn rtmp_event_channel() -> (RtmpEventSender, RtmpEventReceiver) {
    let state = ChannelState {
        sender_count: 1,
        receiver_alive: true,
        ..Default::default()
    };
    let shared = Arc::new((Mutex::new(state), Condvar::new()));
    (
        RtmpEventSender {
            shared: shared.clone(),
        },
        RtmpEventReceiver { shared },
    )
}

impl RtmpEventSender {
    pub fn send(&self, event: RtmpEvent) -> Result<(), RtmpEventSendError> {
        let (lock, cvar) = &*self.shared;
        let mut state = lock.lock().unwrap();

        if !state.receiver_alive {
            return Err(RtmpEventSendError { event });
        }

        state.queue.push_back(event);
        cvar.notify_one();
        Ok(())
    }
}

impl Clone for RtmpEventSender {
    fn clone(&self) -> Self {
        let (lock, _) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.sender_count += 1;

        Self {
            shared: self.shared.clone(),
        }
    }
}

impl Drop for RtmpEventSender {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.sender_count = state.sender_count.saturating_sub(1);
        cvar.notify_all();
    }
}

impl RtmpEventReceiver {
    pub fn recv(&self) -> Option<RtmpEvent> {
        let (lock, cvar) = &*self.shared;
        let mut state = lock.lock().unwrap();

        loop {
            if let Some(event) = state.queue.pop_front() {
                return Some(event);
            }

            if state.sender_count == 0 {
                return None;
            }

            state = cvar.wait(state).unwrap();
        }
    }

    pub fn peek(&self) -> Option<RtmpEvent> {
        let (lock, _) = &*self.shared;
        let state = lock.lock().unwrap();
        state.queue.front().cloned()
    }

    pub fn len(&self) -> usize {
        let (lock, _) = &*self.shared;
        let state = lock.lock().unwrap();
        state.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn first(&self) -> Option<RtmpEvent> {
        self.peek()
    }

    pub fn last(&self) -> Option<RtmpEvent> {
        let (lock, _) = &*self.shared;
        let state = lock.lock().unwrap();
        state.queue.back().cloned()
    }

    pub fn buffer_snapshot(&self) -> RtmpEventBufferSnapshot {
        let (lock, _) = &*self.shared;
        let state = lock.lock().unwrap();
        RtmpEventBufferSnapshot {
            len: state.queue.len(),
            first: state.queue.front().cloned(),
            last: state.queue.back().cloned(),
        }
    }
}

impl Iterator for RtmpEventReceiver {
    type Item = RtmpEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
    }
}

impl Drop for RtmpEventReceiver {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.receiver_alive = false;
        state.queue.clear();
        cvar.notify_all();
    }
}
