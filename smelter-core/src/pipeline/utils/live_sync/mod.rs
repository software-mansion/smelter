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
//! written to it. Each handle is independently owned, so tracks can be
//! processed on separate threads.
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
//! Every input runs one estimator per track plus a shared one observing the
//! chunks of all tracks; all of them keep observing for the whole lifetime of
//! the input. A track starts once both its own and the shared estimate are
//! stable (`max_wait` and `max_hold` bound the wait as safety valves) and
//! picks the edge to align to:
//! - the shared estimate (defined by the freshest track) when it lies within
//!   `shared_edge_tolerance` of the track's own estimate,
//! - the track's own estimate otherwise; the distance means the track's
//!   timestamp space is unrelated to the other tracks (e.g. a different pts
//!   baseline), so the shared edge does not map onto its timestamps.
//!
//! The start maps the chosen edge to `start_margin + target buffer` after the
//! start moment; older backlog maps before the start point and plays late or
//! is dropped by the consumer. The mapping depends only on the chosen edge,
//! not on when the track started, so tracks that agree on the shared edge end
//! up mutually in sync even though each starts on its own.
//! The target buffer is `desired_buffer`, raised for batched delivery to
//! survive the observed gap between batches
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
use state::SharedState;

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncOptions {
    /// Steady-state buffer (time between a chunk arriving and the moment it is
    /// needed for playback) targeted when a track starts.
    pub desired_buffer: Duration,
    /// How long the live edge estimates have to stay stable before starting.
    pub stabilization_period: Duration,
    /// Estimate improvements smaller than this (delivery jitter) do not reset
    /// the stabilization timer.
    pub stabilization_tolerance: Duration,
    /// Extra delay added at start; gives decoders time to process the first
    /// chunks before they are needed for playback.
    pub start_margin: Duration,
    /// Start with the current estimates if the live edge was not detected
    /// within this much time from the track's first chunk.
    pub max_wait: Duration,
    /// Start with the current estimates if more than this much content gets
    /// buffered while waiting for the live edge.
    pub max_hold: Duration,
    /// A track aligns to the shared (all tracks) live edge estimate when it
    /// is within this distance of the track's own estimate; otherwise the
    /// track's timestamp space is considered unrelated to the other tracks
    /// and its own estimate is used.
    pub shared_edge_tolerance: Duration,
}

impl LiveSyncOptions {
    pub fn with_desired_buffer(desired_buffer: Duration) -> Self {
        Self {
            desired_buffer,
            stabilization_period: Duration::from_secs(2),
            stabilization_tolerance: Duration::from_millis(200),
            start_margin: Duration::from_millis(500),
            max_wait: desired_buffer + Duration::from_secs(8),
            max_hold: Duration::from_secs(20).max(desired_buffer * 4),
            shared_edge_tolerance: Duration::from_secs(10),
        }
    }
}

/// Synchronization of a single input; create per-track handles with
/// [`LiveSync::add_track`].
pub(crate) struct LiveSync {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    shared: Arc<Mutex<SharedState>>,
}

impl LiveSync {
    pub fn new(options: LiveSyncOptions, sync_point: Instant) -> Self {
        Self {
            options,
            sync_point,
            shared: Arc::new(Mutex::new(SharedState {
                shared_estimator: LiveEdgeEstimator::new(
                    sync_point,
                    options.stabilization_tolerance,
                ),
                flushed: false,
            })),
        }
    }

    /// Registers a new track. Tracks share the live edge detection but each
    /// starts on its own.
    pub fn add_track<T: InputSyncItem>(&self) -> LiveSyncTrack<T> {
        LiveSyncTrack::new(self.options, self.sync_point, self.shared.clone())
    }

    /// Give up on live edge detection; each track releases everything it
    /// buffered on its next call (e.g. when the stream ended before the live
    /// edge was detected).
    pub fn flush(&self) {
        self.shared.lock().unwrap().flushed = true;
    }
}
