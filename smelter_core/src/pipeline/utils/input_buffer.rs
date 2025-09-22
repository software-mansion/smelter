use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

enum InputBuffer {
    Fixed { buffer: Duration },
    LatencyOptimized(Arc<Mutex<LatencyOptimizedBuffer>>),
}

impl InputBuffer {
    fn pts_with_buffer(&self, pts: Duration) -> Duration {
        match self {
            InputBuffer::Fixed { buffer } => pts + *buffer,
            InputBuffer::LatencyOptimized(buffer) => buffer.lock().unwrap().pts_with_buffer(pts),
        }
    }
}

struct LatencyOptimizedBuffer {
    sync_point: Instant,
    /// We expect pts to be at least greater than sync_point.elapsed() + desired_buffer.
    ///
    /// This buffer should be large enough, so a packet can be decoded and
    /// placed in queue before queue attempts to render that pts.
    desired_buffer: Duration,

    minimal_buffer: Duration,

    dynamic_buffer: Duration,
}

impl LatencyOptimizedBuffer {
    fn new(sync_point: Instant) -> Self {
        Self { sync_point, desired_buffer: Duration::from_millis(80), minimal_buffer: Duration::from_millis(20), dynamic_buffer: () }
    }

    fn pts_with_buffer(&mut self, pts: Duration) -> Duration {
        const INCREMENT_DURATION: Duration = Duration::from_micros(100);
        let next_pts = pts + self.dynamic_buffer;
        if next_pts > self.sync_point.elapsed() + self.desired_buffer {
            // on time
        } else if next_pts > self.sync_point.elapsed() {
            self.dynamic_buffer += 
        } else {
            self.dynamic_buffer =
                (self.sync_point.elapsed() + self.desired_buffer).saturating_sub(pts);
        }

        pts + self.dynamic_buffer
    }
}
