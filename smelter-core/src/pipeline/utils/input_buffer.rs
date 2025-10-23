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

    pub fn pts_with_buffer(&self, pts: Duration) -> Duration {
        match self {
            InputBuffer::None => pts,
            InputBuffer::Const { buffer } => pts + *buffer,
            InputBuffer::LatencyOptimized(buffer) => buffer.lock().unwrap().pts_with_buffer(pts),
            InputBuffer::Adaptive(buffer) => buffer.lock().unwrap().pts_with_buffer(pts),
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
        }
    }

    fn pts_with_buffer(&mut self, pts: Duration) -> Duration {
        const INCREMENT_DURATION: Duration = Duration::from_micros(1000);
        const DECREMENT_DURATION: Duration = Duration::from_micros(100);
        const STABLE_STATE_DURATION: Duration = Duration::from_secs(10);

        let next_pts = pts + self.dynamic_buffer;
        if next_pts > self.sync_point.elapsed() + self.max_buffer {
            let first_pts = self.state.set_over_max_buffer(next_pts);
            if next_pts.saturating_sub(first_pts) > STABLE_STATE_DURATION {
                debug!(
                    old=?self.dynamic_buffer,
                    new=?self.dynamic_buffer.saturating_sub(DECREMENT_DURATION),
                    "Decreased latency optimized buffer"
                );
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

        trace!(
            next_pts=?pts+self.dynamic_buffer,
            queue_pts=?self.sync_point.elapsed(),
            "latency optimized buffer next packet"
        );
        pts + self.dynamic_buffer
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

    fn pts_with_buffer(&mut self, pts: Duration) -> Duration {
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

        pts + self.dynamic_buffer
    }
}
