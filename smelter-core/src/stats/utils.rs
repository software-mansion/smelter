use std::{
    collections::VecDeque,
    iter::Sum,
    time::{Duration, Instant},
};

/// Trait that captures the per-type aggregation semantics
/// `SlidingWindowValue` needs (min/max/avg). Implemented for the few numeric
/// types we actually store. Lets the window support `f64` drift values
/// alongside `Duration` and integers without running into coherence problems
/// (`f64` lacks `Ord` and native `Div<u32>`).
pub trait Aggregable: Copy + Default {
    fn agg_max(a: Self, b: Self) -> Self;
    fn agg_min(a: Self, b: Self) -> Self;
    fn agg_div(self, count: u32) -> Self;
}

impl Aggregable for Duration {
    fn agg_max(a: Self, b: Self) -> Self {
        Self::max(a, b)
    }
    fn agg_min(a: Self, b: Self) -> Self {
        Self::min(a, b)
    }
    fn agg_div(self, count: u32) -> Self {
        self / count
    }
}

impl Aggregable for u32 {
    fn agg_max(a: Self, b: Self) -> Self {
        Self::max(a, b)
    }
    fn agg_min(a: Self, b: Self) -> Self {
        Self::min(a, b)
    }
    fn agg_div(self, count: u32) -> Self {
        self / count
    }
}

impl Aggregable for u64 {
    fn agg_max(a: Self, b: Self) -> Self {
        Self::max(a, b)
    }
    fn agg_min(a: Self, b: Self) -> Self {
        Self::min(a, b)
    }
    fn agg_div(self, count: u32) -> Self {
        self / count as u64
    }
}

impl Aggregable for f64 {
    fn agg_max(a: Self, b: Self) -> Self {
        Self::max(a, b)
    }
    fn agg_min(a: Self, b: Self) -> Self {
        Self::min(a, b)
    }
    fn agg_div(self, count: u32) -> Self {
        self / count as f64
    }
}

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

impl<Value: Aggregable> SlidingWindowValue<Value> {
    pub fn max(&mut self) -> Value {
        let now = Instant::now();
        self.drop_older(now);
        self.buffer
            .iter()
            .map(|(_, v)| *v)
            .reduce(Value::agg_max)
            .unwrap_or_default()
    }

    pub fn min(&mut self) -> Value {
        let now = Instant::now();
        self.drop_older(now);
        self.buffer
            .iter()
            .map(|(_, v)| *v)
            .reduce(Value::agg_min)
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

impl<Value: Sum + Aggregable> SlidingWindowValue<Value> {
    pub fn avg(&mut self) -> Value {
        let now = Instant::now();
        self.drop_older(now);
        if self.buffer.is_empty() {
            return Value::default();
        }
        let sum: Value = self.buffer.iter().map(|(_, v)| *v).sum();
        sum.agg_div(self.buffer.len() as u32)
    }
}
