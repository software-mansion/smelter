use std::{
    fmt,
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
    state: LatencyOptimizedBufferState,
    dynamic_buffer: Duration,

    /// effective_buffer = next_pts - queue_sync_point.elapsed()
    /// Estimates how much time packet has to reach the queue.

    /// If effective_buffer is above this threshold for a period of time, aggressively shrink
    /// the buffer.
    max_hard_threshold: Duration,
    /// If effective_buffer is above this threshold for a period of time, slowly shrink the buffer.
    max_soft_threshold: Duration,
    /// If effective_buffer is below this value, slowly increase the buffer with every packet.
    desired_buffer: Duration,
    /// If effective_buffer is below this threshold, aggressively and immediately increase the buffer.
    min_threshold: Duration,
}

impl LatencyOptimizedBuffer {
    fn new(ctx: &PipelineCtx) -> Self {
        // As a result for default numbers if effective_buffer is between 80ms and 240ms, no
        // adjustment/optimization will be triggered
        let min_threshold = ctx.default_buffer_duration;
        let desired_buffer = min_threshold + ctx.default_buffer_duration;
        let max_soft_threshold = desired_buffer + ctx.default_buffer_duration;
        let max_hard_threshold = max_soft_threshold + Duration::from_millis(500);
        Self {
            sync_point: ctx.queue_sync_point,
            dynamic_buffer: ctx.default_buffer_duration,
            state: LatencyOptimizedBufferState::Ok,

            min_threshold,
            desired_buffer,
            max_soft_threshold,
            max_hard_threshold,
        }
    }

    fn recalculate_buffer(&mut self, pts: Duration) {
        const INCREMENT_DURATION: Duration = Duration::from_micros(200);
        const DECREMENT_DURATION: Duration = Duration::from_micros(200);
        const STABLE_STATE_DURATION: Duration = Duration::from_secs(10);

        let next_pts = pts + self.dynamic_buffer;
        trace!(effective_buffer=?next_pts.saturating_sub(self.sync_point.elapsed()));

        if next_pts > self.sync_point.elapsed() + self.max_hard_threshold {
            let first_pts = self.state.set_too_large(next_pts);
            if next_pts.saturating_sub(first_pts) > STABLE_STATE_DURATION {
                self.dynamic_buffer = self
                    .dynamic_buffer
                    .saturating_sub(self.dynamic_buffer / 100);
            }
        } else if next_pts > self.sync_point.elapsed() + self.max_soft_threshold {
            let first_pts = self.state.set_too_large(next_pts);
            if next_pts.saturating_sub(first_pts) > STABLE_STATE_DURATION {
                self.dynamic_buffer = self.dynamic_buffer.saturating_sub(DECREMENT_DURATION);
            }
        } else if next_pts > self.sync_point.elapsed() + self.desired_buffer {
            self.state.set_ok();
        } else if next_pts > self.sync_point.elapsed() + self.min_threshold {
            trace!(
                old=?self.dynamic_buffer,
                new=?self.dynamic_buffer + INCREMENT_DURATION,
                "Increase latency optimized buffer"
            );
            self.state.set_too_small();
            self.dynamic_buffer += INCREMENT_DURATION;
        } else {
            let new_buffer = (self.sync_point.elapsed() + self.desired_buffer).saturating_sub(pts);
            debug!(
                old=?self.dynamic_buffer,
                new=?new_buffer,
                "Increase latency optimized buffer (force)"
            );
            self.state.set_too_small();
            // adjust buffer so:
            // pts + self.dynamic_buffer == self.sync_point.elapsed() + self.desired_buffer
            self.dynamic_buffer = new_buffer
        }
    }
}

enum LatencyOptimizedBufferState {
    Ok,
    TooSmall,
    TooLarge { first_pts: Duration },
}

impl LatencyOptimizedBufferState {
    fn set_too_large(&mut self, pts: Duration) -> Duration {
        match &self {
            LatencyOptimizedBufferState::TooLarge { first_pts } => *first_pts,
            _ => {
                *self = LatencyOptimizedBufferState::TooLarge { first_pts: pts };
                pts
            }
        }
    }

    fn set_too_small(&mut self) {
        *self = LatencyOptimizedBufferState::TooSmall
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
