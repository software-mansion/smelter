use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use crate::utils::TimedValue;

struct Shared<T> {
    inner: Mutex<Inner<T>>,
    /// Signaled when an item is pushed (receiver waits on this).
    not_empty: Condvar,
    /// Signaled when an item is popped (sender waits on this).
    not_full: Condvar,
}

struct Inner<T> {
    buffer: VecDeque<T>,
    capacity: Duration,
    sender_count: usize,
    receiver_alive: bool,
}

impl<T: TimedValue> Inner<T> {
    fn buffered_duration(&self) -> Duration {
        let first_ts = self.buffer.iter().find_map(|i| i.timestamp_range());
        let last_ts = self.buffer.iter().rev().find_map(|i| i.timestamp_range());
        match (first_ts, last_ts) {
            (Some(first), Some(last)) => last.1.saturating_sub(first.0),
            _ => Duration::ZERO,
        }
    }

    fn is_full(&self) -> bool {
        self.buffered_duration() >= self.capacity
    }

    fn push(&mut self, item: T) {
        self.buffer.push_back(item);
    }

    fn pop(&mut self) -> Option<T> {
        self.buffer.pop_front()
    }
}

pub(crate) fn duration_bounded<T: TimedValue>(capacity: Duration) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        inner: Mutex::new(Inner {
            buffer: VecDeque::new(),
            capacity,
            sender_count: 1,
            receiver_alive: true,
        }),
        not_empty: Condvar::new(),
        not_full: Condvar::new(),
    });
    (
        Sender {
            shared: shared.clone(),
        },
        Receiver { shared },
    )
}

// ── Sender ──────────────────────────────────────────────────────────

pub(crate) struct Sender<T> {
    shared: Arc<Shared<T>>,
}

impl<T> std::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("duration_channel::Sender").finish()
    }
}

impl<T: TimedValue> Sender<T> {
    /// Blocks until there is room or the receiver is dropped.
    pub fn send(&self, item: T) -> Result<(), SendError<T>> {
        let mut guard = self.shared.inner.lock().unwrap();
        loop {
            if !guard.receiver_alive {
                return Err(SendError(item));
            }
            if !guard.is_full() {
                guard.push(item);
                self.shared.not_empty.notify_one();
                return Ok(());
            }
            guard = self.shared.not_full.wait(guard).unwrap();
        }
    }

    /// Non-blocking send.
    #[allow(dead_code)]
    pub fn try_send(&self, item: T) -> Result<(), TrySendError<T>> {
        let mut guard = self.shared.inner.lock().unwrap();
        if !guard.receiver_alive {
            return Err(TrySendError::Disconnected(item));
        }
        if guard.is_full() {
            return Err(TrySendError::Full(item));
        }
        guard.push(item);
        self.shared.not_empty.notify_one();
        Ok(())
    }

    /// Blocks until there is room, the receiver is dropped, or the timeout elapses.
    pub fn send_timeout(&self, item: T, timeout: Duration) -> Result<(), SendTimeoutError<T>> {
        let deadline = Instant::now() + timeout;
        let mut guard = self.shared.inner.lock().unwrap();
        loop {
            if !guard.receiver_alive {
                return Err(SendTimeoutError::Disconnected(item));
            }
            if !guard.is_full() {
                guard.push(item);
                self.shared.not_empty.notify_one();
                return Ok(());
            }
            guard = {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(SendTimeoutError::Timeout(item));
                }
                let (guard, timeout_result) =
                    self.shared.not_full.wait_timeout(guard, remaining).unwrap();
                if !guard.receiver_alive {
                    return Err(SendTimeoutError::Disconnected(item));
                }
                if timeout_result.timed_out() && guard.is_full() {
                    return Err(SendTimeoutError::Timeout(item));
                }
                guard
            }
        }
    }

    pub fn buffered_duration(&self) -> Duration {
        self.shared.inner.lock().unwrap().buffered_duration()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.inner.lock().unwrap().sender_count += 1;
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut inner = self.shared.inner.lock().unwrap();
        inner.sender_count -= 1;
        if inner.sender_count == 0 {
            self.shared.not_empty.notify_all();
        }
    }
}

// ── Receiver ────────────────────────────────────────────────────────

pub(crate) struct Receiver<T> {
    shared: Arc<Shared<T>>,
}

impl<T: TimedValue> Receiver<T> {
    /// Blocks until an item is available or all senders are dropped.
    pub fn recv(&self) -> Result<T, RecvError> {
        let mut guard = self.shared.inner.lock().unwrap();
        loop {
            if let Some(item) = guard.pop() {
                self.shared.not_full.notify_one();
                return Ok(item);
            }
            if guard.sender_count == 0 {
                return Err(RecvError);
            }
            guard = self.shared.not_empty.wait(guard).unwrap();
        }
    }

    /// Non-blocking receive.
    #[allow(dead_code)]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut guard = self.shared.inner.lock().unwrap();
        if let Some(item) = guard.pop() {
            self.shared.not_full.notify_one();
            return Ok(item);
        }
        if guard.sender_count == 0 {
            return Err(TryRecvError::Disconnected);
        }
        Err(TryRecvError::Empty)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut guard = self.shared.inner.lock().unwrap();
        guard.receiver_alive = false;
        self.shared.not_full.notify_all();
    }
}

impl<T: TimedValue> IntoIterator for Receiver<T> {
    type Item = T;
    type IntoIter = DurationReceiverIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        DurationReceiverIter { receiver: self }
    }
}

pub(crate) struct DurationReceiverIter<T> {
    receiver: Receiver<T>,
}

impl<T: TimedValue> Iterator for DurationReceiverIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.receiver.recv().ok()
    }
}

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SendError<T>(pub T);

#[derive(Debug)]
pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}

#[derive(Debug)]
pub enum SendTimeoutError<T> {
    Timeout(T),
    Disconnected(T),
}

#[derive(Debug)]
pub(crate) struct RecvError;

#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct TestItem {
        start: Duration,
        end: Duration,
        value: u32,
    }

    impl TimedValue for TestItem {
        fn timestamp_range(&self) -> Option<(Duration, Duration)> {
            Some((self.start, self.end))
        }
    }

    fn item(start_ms: u64, end_ms: u64, value: u32) -> TestItem {
        TestItem {
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(end_ms),
            value,
        }
    }

    #[test]
    fn send_recv_basic() {
        let (tx, rx) = duration_bounded(Duration::from_millis(100));
        tx.send(item(0, 30, 1)).unwrap();
        tx.send(item(30, 60, 2)).unwrap();
        assert_eq!(rx.recv().unwrap().value, 1);
        assert_eq!(rx.recv().unwrap().value, 2);
    }

    #[test]
    fn try_send_returns_full_when_capacity_exceeded() {
        // capacity=50ms; after two items span is 0..60 = 60ms >= 50ms, so channel is full
        let (tx, _rx) = duration_bounded(Duration::from_millis(50));
        tx.try_send(item(0, 30, 1)).unwrap();
        tx.try_send(item(30, 60, 2)).unwrap();
        match tx.try_send(item(60, 90, 3)) {
            Err(TrySendError::Full(_)) => {}
            other => panic!("expected Full, got {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn send_blocks_until_recv_frees_capacity() {
        // capacity=50ms; after two items span is 0..60 = 60ms >= 50ms
        let (tx, rx) = duration_bounded(Duration::from_millis(50));
        tx.send(item(0, 30, 1)).unwrap();
        tx.send(item(30, 60, 2)).unwrap();

        let handle = std::thread::spawn(move || {
            // This blocks because span 0..90 = 90ms > 50ms capacity
            tx.send(item(60, 90, 3)).unwrap();
        });

        std::thread::sleep(Duration::from_millis(50));
        // Popping item(0,30) makes span 30..60 = 30ms < 50ms, unblocking sender
        assert_eq!(rx.recv().unwrap().value, 1);
        handle.join().unwrap();
        assert_eq!(rx.recv().unwrap().value, 2);
        assert_eq!(rx.recv().unwrap().value, 3);
    }

    #[test]
    fn recv_returns_error_when_all_senders_dropped() {
        let (tx, rx) = duration_bounded::<TestItem>(Duration::from_secs(1));
        drop(tx);
        assert!(rx.recv().is_err());
    }

    #[test]
    fn send_returns_error_when_receiver_dropped() {
        let (tx, rx) = duration_bounded(Duration::from_secs(1));
        drop(rx);
        assert!(tx.send(item(0, 10, 1)).is_err());
    }

    #[test]
    fn into_iter_yields_until_senders_drop() {
        let (tx, rx) = duration_bounded(Duration::from_secs(1));
        tx.send(item(0, 10, 1)).unwrap();
        tx.send(item(10, 20, 2)).unwrap();
        drop(tx);
        let values: Vec<u32> = rx.into_iter().map(|i| i.value).collect();
        assert_eq!(values, vec![1, 2]);
    }

    #[test]
    fn clone_sender_keeps_channel_alive() {
        let (tx, rx) = duration_bounded(Duration::from_secs(1));
        let tx2 = tx.clone();
        drop(tx);
        tx2.send(item(0, 10, 1)).unwrap();
        assert_eq!(rx.recv().unwrap().value, 1);
        drop(tx2);
        assert!(rx.recv().is_err());
    }
}
