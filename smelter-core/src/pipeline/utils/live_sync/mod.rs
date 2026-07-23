//! Live-edge synchronization for live inputs (RTMP, HLS, MoQ).
//!
//! Live protocols rarely deliver data at a real time rate right after
//! connecting. RTMP clients can flush a few seconds of pre-buffered chunks,
//! HLS delivers whole segments in batches. If playback timing is decided when
//! the connection is established, that initial backlog ends up stretched,
//! squashed or dropped by the consumer.
//!
//! [`LiveSync`] represents one input; [`LiveSync::add_track`] returns a
//! [`LiveSyncTrack`] handle per track (video, audio) that buffers chunks
//! written to it. Tracks share the live edge detection and start producing
//! chunks at the same moment, but each handle is independently owned, so
//! tracks can be processed on separate threads.
//!
//! Live edge detection is implemented by [`LiveEdgeEstimator`] (usable on its
//! own by inputs with different buffering logic):
//! - For every chunk it samples `offset = arrival_time - pts`. The smallest
//!   observed offset corresponds to the freshest content seen so far and is
//!   used as the live edge estimate.
//! - When the estimate stops improving for `stabilization_period`, delivery
//!   reached a real time rate and the estimate is considered final. This works
//!   for batched delivery too: the end of each batch is the freshest sample,
//!   so the estimate plateaus between batches regardless of the batch size.
//!
//! Based on the estimate [`LiveSync`] decides how to start:
//!   - Buffer reasonably close to `desired_buffer` (between `min_start_buffer`
//!     and `max_start_buffer`): start with everything that is buffered, no
//!     data is dropped and no extra waiting happens.
//!   - Too much buffered: trim the front (respecting track start points, e.g.
//!     video keyframes) so the steady-state buffer is close to
//!     `desired_buffer`.
//!   - Too little buffered: keep waiting until the buffer grows to
//!     `desired_buffer`.
//! - Safety valves: `max_wait` bounds the total wait time and `max_hold`
//!   bounds the buffered content; when exceeded playback starts immediately
//!   with the current estimate.
//!
//! The configured buffer sizes are treated as floors: for batched delivery
//! the effective sizes are raised to survive the observed gap between batches
//! (`LiveEdgeEstimate::max_arrival_gap`), so the sync works even when the
//! batch size (e.g. HLS segment duration) is unknown or unexpected.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::pipeline::utils::input_sync::InputSyncItem;

mod edge_estimator;
mod state;
mod track;

pub(crate) use track::LiveSyncTrack;

use edge_estimator::LiveEdgeEstimator;
use state::{SharedState, StartDetection};

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncOptions {
    /// Steady-state buffer (time between a chunk arriving and the moment it is
    /// needed for playback) targeted when the sync trims or waits.
    pub desired_buffer: Duration,
    /// Start is delayed until at least this much content is available.
    pub min_start_buffer: Duration,
    /// Content above this limit is trimmed at start down to `desired_buffer`.
    pub max_start_buffer: Duration,
    /// How long the live edge estimate has to stay stable before starting.
    pub stabilization_period: Duration,
    /// Estimate improvements smaller than this (delivery jitter) do not reset
    /// the stabilization timer.
    pub stabilization_tolerance: Duration,
    /// Extra delay added at start; gives decoders time to process the first
    /// chunks before they are needed for playback.
    pub start_margin: Duration,
    /// Start with the current estimate if the live edge was not detected
    /// within this much time from the first chunk.
    pub max_wait: Duration,
    /// Start with the current estimate if more than this much content gets
    /// buffered while waiting for the live edge.
    pub max_hold: Duration,
}

impl LiveSyncOptions {
    pub fn with_desired_buffer(desired_buffer: Duration) -> Self {
        Self {
            desired_buffer,
            min_start_buffer: desired_buffer / 2,
            max_start_buffer: desired_buffer * 3,
            stabilization_period: Duration::from_secs(2),
            stabilization_tolerance: Duration::from_millis(200),
            start_margin: Duration::from_millis(500),
            max_wait: desired_buffer + Duration::from_secs(8),
            max_hold: Duration::from_secs(20).max(desired_buffer * 4),
        }
    }
}

/// Synchronization of a single input; create per-track handles with
/// [`LiveSync::add_track`].
pub(crate) struct LiveSync {
    shared: Arc<SharedState>,
}

impl LiveSync {
    pub fn new(options: LiveSyncOptions, sync_point: Instant) -> Self {
        Self {
            shared: Arc::new(SharedState {
                options,
                sync_point,
                detection: Mutex::new(StartDetection {
                    tracks: Vec::new(),
                    estimator: LiveEdgeEstimator::new(sync_point, options.stabilization_tolerance),
                    filling: false,
                    start: None,
                }),
            }),
        }
    }

    /// Registers a new track. All tracks share the live edge detection and
    /// start at the same moment.
    pub fn add_track<T: InputSyncItem>(&self) -> LiveSyncTrack<T> {
        LiveSyncTrack::new(self.shared.clone())
    }

    /// Give up on live edge detection and release everything that is buffered
    /// (e.g. when the stream ended before the live edge was detected).
    pub fn flush(&self) {
        let mut detection = self.shared.detection.lock().unwrap();
        if detection.start.is_none() {
            detection.start_now(&self.shared, Instant::now(), None, "flush");
        }
    }
}
