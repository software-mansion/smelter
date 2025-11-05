use std::{
    fmt,
    ops::{Add, Div},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::{debug, info, trace};

use crate::{InputBufferOptions, PipelineCtx};

#[derive(Clone)]
pub(crate) enum InputBuffer {
    // If input is required or has an offset do not add any buffer.
    //
    // - If input is required, buffering is not necessary.
    // - If offset is in the future then extra buffering is not necessary
    // - If offset in in the past, it already causing drops
    None,
    Const { buffer: Duration },
    LatencyOptimized(Arc<Mutex<LatencyOptimizedBuffer>>),
    Adaptive(Arc<Mutex<AdaptiveBuffer>>),
}

impl fmt::Debug for InputBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Const { buffer } => f.debug_struct("Const").field("buffer", buffer).finish(),
            Self::LatencyOptimized(_) => f.debug_struct("LatencyOptimized").finish(),
            Self::Adaptive(_) => f.debug_struct("Adaptive").finish(),
        }
    }
}

impl InputBuffer {
    pub fn new(ctx: &PipelineCtx, opts: InputBufferOptions) -> Self {
        match opts {
            InputBufferOptions::None => InputBuffer::None,
            InputBufferOptions::Const(buffer) => InputBuffer::Const {
                buffer: buffer.unwrap_or(ctx.default_buffer_duration),
            },
            InputBufferOptions::LatencyOptimized => InputBuffer::LatencyOptimized(Arc::new(
                Mutex::new(LatencyOptimizedBuffer::new(ctx)),
            )),
            InputBufferOptions::Adaptive => {
                InputBuffer::Adaptive(Arc::new(Mutex::new(AdaptiveBuffer::new(ctx))))
            }
        }
    }

    pub fn recalculate_buffer(&self, pts: Duration) {
        match self {
            InputBuffer::LatencyOptimized(buffer) => buffer.lock().unwrap().recalculate_buffer(pts),
            InputBuffer::Adaptive(buffer) => buffer.lock().unwrap().recalculate_buffer(pts),
            _ => (),
        }
    }

    pub fn size(&self) -> Duration {
        match self {
            InputBuffer::None => Duration::ZERO,
            InputBuffer::Const { buffer } => *buffer,
            InputBuffer::LatencyOptimized(buffer) => buffer.lock().unwrap().dynamic_buffer,
            InputBuffer::Adaptive(buffer) => buffer.lock().unwrap().dynamic_buffer,
        }
    }
}

/// Buffer intended for low latency inputs, if input stream is not delivered on time,
/// it quickly increases. However, when buffer is stable for some time it starts to shrink to
/// minimize the latency.
pub(crate) struct LatencyOptimizedBuffer {
    sync_point: Instant,
    /// We expect pts to be at least greater than sync_point.elapsed() + desired_buffer.
    ///
    /// This buffer should be large enough, so a packet can be decoded and
    /// placed in queue before queue attempts to render that pts.
    desired_buffer: Duration,

    min_buffer: Duration,
    max_buffer: Duration,

    dynamic_buffer: Duration,

    state: LatencyOptimizedBufferState,
    effective_buffer_log: ThrottledLogger<Duration>,
}

impl LatencyOptimizedBuffer {
    fn new(ctx: &PipelineCtx) -> Self {
        Self {
            sync_point: ctx.queue_sync_point,
            desired_buffer: ctx.default_buffer_duration,
            min_buffer: Duration::min(Duration::from_millis(20), ctx.default_buffer_duration),
            max_buffer: ctx.default_buffer_duration + Duration::from_millis(50),
            dynamic_buffer: ctx.default_buffer_duration,
            state: LatencyOptimizedBufferState::Ok,
            effective_buffer_log: ThrottledLogger::new(Box::new(|min, max, avg| {
                debug!(?min, ?max, ?avg, "Effective jitter buffer")
            })),
        }
    }

    fn recalculate_buffer(&mut self, pts: Duration) {
        const INCREMENT_DURATION: Duration = Duration::from_micros(100);
        const DECREMENT_DURATION: Duration = Duration::from_micros(10);
        const STABLE_STATE_DURATION: Duration = Duration::from_secs(10);

        let next_pts = pts + self.dynamic_buffer;
        trace!(effective_buffer=?next_pts.saturating_sub(self.sync_point.elapsed()));
        if next_pts > self.sync_point.elapsed() + self.max_buffer {
            let first_pts = self.state.set_over_max_buffer(next_pts);
            if next_pts.saturating_sub(first_pts) > STABLE_STATE_DURATION {
                self.dynamic_buffer = self.dynamic_buffer.saturating_sub(DECREMENT_DURATION);
            }
        } else if next_pts > self.sync_point.elapsed() + self.desired_buffer {
            self.state.set_ok();
        } else if next_pts > self.sync_point.elapsed() + self.min_buffer {
            trace!(
                old=?self.dynamic_buffer,
                new=?self.dynamic_buffer + INCREMENT_DURATION,
                "Increase latency optimized buffer"
            );
            self.state.set_to_small();
            self.dynamic_buffer += INCREMENT_DURATION;
        } else {
            let new_buffer = (self.sync_point.elapsed() + self.desired_buffer).saturating_sub(pts);
            debug!(
                old=?self.dynamic_buffer,
                new=?new_buffer,
                "Increase latency optimized buffer (force)"
            );
            self.state.set_to_small();
            // adjust buffer so:
            // pts + self.dynamic_buffer == self.sync_point.elapsed() + self.desired_buffer
            self.dynamic_buffer = new_buffer
        }
    }
}

enum LatencyOptimizedBufferState {
    Ok,
    ToSmallBuffer,
    OverMaxBuffer { first_pts: Duration },
}

impl LatencyOptimizedBufferState {
    fn set_over_max_buffer(&mut self, pts: Duration) -> Duration {
        match &self {
            LatencyOptimizedBufferState::OverMaxBuffer { first_pts } => *first_pts,
            _ => {
                *self = LatencyOptimizedBufferState::OverMaxBuffer { first_pts: pts };
                pts
            }
        }
    }

    fn set_to_small(&mut self) {
        *self = LatencyOptimizedBufferState::ToSmallBuffer
    }

    fn set_ok(&mut self) {
        *self = LatencyOptimizedBufferState::Ok
    }
}

pub(crate) struct AdaptiveBuffer {
    sync_point: Instant,
    desired_buffer: Duration,
    dynamic_buffer: Duration,
    min_buffer: Duration,
}

impl AdaptiveBuffer {
    fn new(ctx: &PipelineCtx) -> Self {
        Self {
            sync_point: ctx.queue_sync_point,
            desired_buffer: ctx.default_buffer_duration,
            min_buffer: Duration::min(Duration::from_millis(20), ctx.default_buffer_duration),
            dynamic_buffer: ctx.default_buffer_duration,
        }
    }

    fn recalculate_buffer(&mut self, pts: Duration) {
        const INCREMENT_DURATION: Duration = Duration::from_micros(100);

        let next_pts = pts + self.dynamic_buffer;
        if next_pts > self.sync_point.elapsed() + self.desired_buffer {
            // ok
        } else if next_pts > self.sync_point.elapsed() + self.min_buffer {
            debug!(
                old=?self.dynamic_buffer,
                new=?self.dynamic_buffer + INCREMENT_DURATION,
                "Increase adaptive buffer"
            );
            self.dynamic_buffer += INCREMENT_DURATION;
        } else {
            let new_buffer = (self.sync_point.elapsed() + self.desired_buffer).saturating_sub(pts);
            info!(
                old=?self.dynamic_buffer,
                new=?new_buffer,
                "Increase adaptive buffer (force)"
            );
            self.dynamic_buffer = new_buffer
        }
    }
}

struct ThrottledLogger<V: PartialOrd + Copy + Add<Output = V> + Default + Div<u32, Output = V>> {
    max: Option<V>,
    min: Option<V>,
    sum: V,
    count: u32,
    last: Instant,
    log_fn: Box<dyn FnMut(V, V, V)>,
}

impl<V: PartialOrd + Copy + Add<Output = V> + Default + Div<u32, Output = V>> ThrottledLogger<V> {
    fn new(log_fn: Box<dyn FnMut(V, V, V)>) -> Self {
        Self {
            max: None,
            min: None,
            sum: V::default(),
            count: 0,
            last: Instant::now(),
            log_fn,
        }
    }

    fn log(&mut self, value: V) {
        let max = match self.max {
            Some(max) if max < value => {
                self.max = Some(value);
                value
            }
            Some(max) => max,
            None => {
                self.max = Some(value);
                value
            }
        };
        let min = match self.min {
            Some(min) if min < value => {
                self.min = Some(value);
                value
            }
            Some(min) => min,
            None => {
                self.min = Some(value);
                value
            }
        };
        self.sum = self.sum + value;
        self.count += 1;

        if self.last.elapsed() > Duration::from_secs(1) {
            let avg = self.sum / self.count;
            self.last = Instant::now();
            self.max = None;
            self.min = None;
            self.count = 0;
            self.sum = V::default();
            (self.log_fn)(min, max, avg)
        }
    }
}
