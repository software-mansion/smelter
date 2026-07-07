//! Live-edge synchronization for live inputs (RTMP, HLS, MoQ).
//!
//! Live protocols rarely deliver data at a real time rate right after
//! connecting. RTMP clients can flush a few seconds of pre-buffered chunks,
//! HLS delivers whole segments in batches. Timing playback by arrival alone
//! would stretch, squash or drop that backlog.
//!
//! Chunks written to an input are held back until its live edge has been
//! estimated; that estimate decides where playback starts, far enough behind
//! the edge to keep the configured buffer. The edge is estimated per track
//! and over all tracks at once, because the tracks of an input do not have to
//! share a timeline: one whose timestamps turn out to be unrelated to the
//! other's starts on its own estimate instead of the shared one.
//!
//! Estimation continues after the start, so an edge that drifted away can be
//! corrected by nudging the anchor that maps input timestamps onto output
//! ones. Tracks sharing an anchor release their chunks in a common pts order,
//! which keeps the timestamps they produce advancing together and makes a
//! nudge move both tracks the same way instead of desynchronizing them. A pts
//! discontinuity is what no correction can absorb: the estimate is dropped
//! and the detection starts over on the new timeline. A track that stops
//! delivering for long enough to run out the content it released goes back
//! to the same decision on its own, so it has to earn its place on the
//! shared timeline again when it comes back.
//!
//! Live edge detection itself is implemented by [`LiveEdgeEstimator`], usable
//! on its own by inputs with different buffering logic.
//!
//! [`LiveEdgeEstimator`]: edge_estimator::LiveEdgeEstimator

use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

mod buffer;
mod edge_estimator;
mod state;
mod track;

pub(crate) use buffer::{BufferingStrategy, ChunkBuffer, LiveSyncBuffer};
pub(crate) use track::LiveSyncTrack;

use crate::pipeline::utils::input_sync::{TrackCallback, TrackKind};
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
            stabilization_period: Duration::from_millis(500),
            stabilization_tolerance: Duration::from_millis(100),
            max_wait: buffering_strategy.desired_buffer() + Duration::from_secs(8),
        }
    }
}

/// Synchronization of a single input; create per-track handles with
/// [`LiveSync::add_track`]. The buffer type decides the buffering policy of
/// the tracks (e.g. [`ChunkBuffer`] for in-order delivery).
pub(crate) struct LiveSync<B: LiveSyncBuffer> {
    shared: Arc<Mutex<SharedState<B>>>,
}

/// How often the internal ticker thread drives time-based transitions. Has
/// to stay well below `MIN_QUEUE_HEADROOM`: when delivery stalls, deadline
/// releases are driven only by the ticker, and its granularity eats into the
/// headroom the released chunks have left.
const TICK_INTERVAL: Duration = Duration::from_millis(20);

impl<B: LiveSyncBuffer> LiveSync<B> {
    pub fn new(options: LiveSyncOptions, sync_point: Instant) -> Self {
        let shared = Arc::new(Mutex::new(SharedState::new(options, sync_point)));
        spawn_tick_thread(Arc::downgrade(&shared));
        Self { shared }
    }

    /// Registers the track of the given kind; `callback` receives its chunks
    /// once they are synchronized. Tracks share the live edge detection but
    /// each starts on its own.
    pub fn add_track(
        &self,
        kind: TrackKind,
        callback: TrackCallback<B::Chunk>,
    ) -> LiveSyncTrack<B> {
        self.shared.lock().unwrap().add_track(kind, callback);
        LiveSyncTrack::new(self.shared.clone(), kind)
    }

    /// Give up on live edge detection; every track releases everything it
    /// buffered (e.g. when the stream ended before the live edge was
    /// detected).
    pub fn flush(&self) {
        self.shared.lock().unwrap().flush();
    }
}

/// Drives time-based transitions (start decisions, corrections, bounded
/// waits) while delivery pauses, pushing releasable chunks to the track
/// callbacks; exits once every handle to the input is dropped.
fn spawn_tick_thread<B: LiveSyncBuffer>(shared: Weak<Mutex<SharedState<B>>>) {
    std::thread::Builder::new()
        .name("Live sync ticker".to_string())
        .spawn(move || {
            loop {
                std::thread::sleep(TICK_INTERVAL);
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                shared.lock().unwrap().tick(Instant::now());
            }
        })
        .unwrap();
}
