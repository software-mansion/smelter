use std::{collections::VecDeque, time::Duration};

use smelter_render::Frame;

// Trait used to estimate duration the item
pub(crate) trait TimedValue {
    fn timestamp_range(&self) -> (Duration, Duration);
}

impl TimedValue for Frame {
    fn timestamp_range(&self) -> (Duration, Duration) {
        (
            self.pts.saturating_sub(Duration::from_millis(10)),
            self.pts + Duration::from_millis(10),
        )
    }
}

/// Buffer specific duration of data before returning first timestamp
pub(crate) struct InputDelayBuffer<T: TimedValue> {
    buffer: VecDeque<T>,
    size: Duration,
    ready: bool,
    end: bool,
}

impl<T: TimedValue> InputDelayBuffer<T> {
    pub fn new(size: Duration) -> Self {
        Self {
            buffer: VecDeque::new(),
            size,
            ready: false,
            end: false,
        }
    }

    pub fn write(&mut self, item: T) {
        self.buffer.push_back(item);
        if !self.ready
            && let (Some(first), Some(last)) = (self.buffer.front(), self.buffer.back())
        {
            self.ready = last.timestamp_range().1.abs_diff(first.timestamp_range().0) > self.size
        }
    }

    pub fn read(&mut self) -> Option<T> {
        match self.ready {
            true => self.buffer.pop_front(),
            false => None,
        }
    }

    pub fn mark_end(&mut self) {
        self.ready = true;
        self.end = true;
    }

    pub fn is_done(&self) -> bool {
        self.end && self.buffer.is_empty()
    }
}
