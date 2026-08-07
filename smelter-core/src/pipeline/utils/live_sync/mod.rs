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
//! - For every chunk it samples `offset = arrival_time - pts`; the recent
//!   extremes of the offset bound the live edge (the minimum yields the upper
//!   bound, extrapolated from the freshest delivery seen; the maximum the
//!   lower one). The window makes the bounds follow changes of the network
//!   latency instead of locking to lifetime extremes.
//! - When the upper bound stops improving for `stabilization_period`,
//!   delivery reached a real time rate (dropped to or below it) and the
//!   estimate is considered ready. This works for batched delivery too: the
//!   silence after a batch is itself the signal, so readiness does not depend
//!   on the batch size. The cost of trusting a pause this quickly (a stall
//!   can look like the edge) is covered by the post-start re-anchoring below.
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
//! The start never drops delivered content: playback is anchored at the
//! oldest buffered chunk when more than the target buffer is available, or
//! scheduled at `edge - target buffer` when the buffer still has to fill up.
//! Only a start forced by `max_hold` trims down to the target, since that
//! limit exists to bound latency. The target buffer is `desired_buffer`,
//! raised for batched delivery to survive the observed gap between batches
//! (`LiveEdgeEstimator::max_arrival_gap`). Tracks that agree on the shared
//! edge converge to the same mapping; they can start with different backlog
//! depths, and the correction below aligns them as the excess drains.
//!
//! After the start each track keeps checking how its delivery behaves
//! relative to the playback position:
//! - the buffer drifting away from the target (source clock drift, delivery
//!   falling behind, excess kept at the start) is corrected by slewing the
//!   anchor in small steps;
//! - the chosen edge improving after the start (a delivery stall mistaken
//!   for the live edge) revokes the start with a single forward jump;
//! - deviations too large to slew and pts discontinuities reset the track
//!   back to the startup logic, so the live edge gets re-estimated from
//!   scratch.
//! The target buffer is `desired_buffer`, raised for batched delivery to
//! survive the observed gap between batches
//! (`LiveEdgeEstimator::max_arrival_gap`), so the sync works even when the
//! batch size (e.g. HLS segment duration) is unknown or unexpected.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

mod buffer;
mod edge_estimator;
mod flush;
mod state;
mod track;

pub(crate) use buffer::{ChunkBuffer, LiveSyncBuffer, BufferingStrategy};
pub(crate) use track::LiveSyncTrack;

use edge_estimator::LiveEdgeEstimator;
use flush::FlushState;
use state::SharedState;

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncOptions {
    pub buffering_strategy: BufferingStrategy,
    /// How long the live edge estimates have to stay stable before starting.
    pub stabilization_period: Duration,
    /// Estimate improvements smaller than this (delivery jitter) do not reset
    /// the stabilization timer.
    pub stabilization_tolerance: Duration,
    /// Start with the current estimates if the live edge was not detected
    /// within this much time from the track's first chunk.
    pub max_wait: Duration,
}

impl LiveSyncOptions {
    pub fn with_desired_buffer(buffering_strategy: BufferingStrategy) -> Self {
        Self {
            buffering_strategy,
            stabilization_period: Duration::from_secs(2),
            stabilization_tolerance: Duration::from_millis(200),
            max_wait: buffering_strategy.desired_buffer() + Duration::from_secs(8),
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
    flush_state: FlushState,
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
                anchor: None,
            })),
            flush_state: FlushState::default(),
        }
    }

    /// Registers a new track; the buffer type decides the buffering policy
    /// (e.g. [`ChunkBuffer`] for in-order delivery). Tracks share the live
    /// edge detection but each starts on its own.
    pub fn add_track<B: LiveSyncBuffer>(&self) -> LiveSyncTrack<B> {
        LiveSyncTrack::new(
            self.options,
            self.sync_point,
            self.shared.clone(),
            self.flush_state.track_state(),
        )
    }

    /// Give up on live edge detection; each track observes the flush once and
    /// releases everything it buffered (e.g. when the stream ended before the
    /// live edge was detected).
    pub fn flush(&self) {
        self.flush_state.flush();
    }
}
