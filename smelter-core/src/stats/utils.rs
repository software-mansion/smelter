use std::{
    collections::VecDeque,
    iter::Sum,
    ops::Div,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct SlidingWindowValue<Value: Copy> {
    window_size: Duration,
    buffer: VecDeque<(Instant, Value)>,
}

impl<Value: Copy> SlidingWindowValue<Value> {
    pub fn new(window_size: Duration) -> Self {
        Self {
            window_size,
            buffer: VecDeque::new(),
        }
    }

    pub fn push(&mut self, val: Value) {
        let now = Instant::now();
        self.drop_older(now);
        self.buffer.push_back((now, val));
    }

    pub fn window_size(&self) -> Duration {
        self.window_size
    }

    fn drop_older(&mut self, instant: Instant) {
        while let Some((first, _)) = self.buffer.front()
            && *first + self.window_size < instant
        {
            self.buffer.pop_front();
        }
    }
}

impl<Value: Ord + Copy + Default> SlidingWindowValue<Value> {
    pub fn max(&mut self) -> Value {
        let now = Instant::now();
        self.drop_older(now);
        self.buffer
            .iter()
            .map(|(_, v)| *v)
            .max()
            .unwrap_or_default()
    }

    pub fn min(&mut self) -> Value {
        let now = Instant::now();
        self.drop_older(now);
        self.buffer
            .iter()
            .map(|(_, v)| *v)
            .min()
            .unwrap_or_default()
    }
}

impl<Value: Sum + Copy> SlidingWindowValue<Value> {
    pub fn sum(&mut self) -> Value {
        let now = Instant::now();
        self.drop_older(now);
        self.buffer.iter().map(|(_, v)| *v).sum()
    }
}

impl<Value: Sum + Copy + Div<u32, Output = Value> + Default> SlidingWindowValue<Value> {
    pub fn avg(&mut self) -> Value {
        let now = Instant::now();
        self.drop_older(now);
        if self.buffer.is_empty() {
            return Value::default();
        }
        let sum: Value = self.buffer.iter().map(|(_, v)| *v).sum();
        sum / self.buffer.len() as u32
    }
}
