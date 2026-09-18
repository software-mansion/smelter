//! Live-edge synchronization for live inputs (RTMP, HLS, MoQ).
//!
//! Live protocols rarely deliver data at a real time rate right after connecting. RTMP clients
//! can flush a few seconds of pre-buffered chunks, HLS delivers whole segments in batches. Timing
//! playback by arrival alone would stretch, squash or drop that backlog.
//!
//! Chunks written to an input are held back until its live edge has been estimated; that estimate
//! decides where playback starts, far enough behind the edge to keep the configured buffer.
//! Estimation continues after the start, so an edge that drifted away is corrected by slewing the
//! anchor that maps input timestamps onto output ones.
//!
//! The tracks of an input do not have to share a timeline, so how they are anchored is an
//! input-wide `Mode`. Live edges further apart than the split threshold mean unrelated
//! timelines, closer than the merge threshold a shared one. A shift smaller than the split
//! threshold is taken at face value, since from timing alone it cannot be told from a change in
//! one track's encode latency.
//!
//! Shared timeline:
//! - Both tracks apply one anchor, sized from an estimate over all chunks.
//! - Chunks are released in a common pts order, so a slew moves both tracks the same way.
//!
//! Unrelated timelines:
//! - The leader (audio whenever it runs) has an anchor sized from its own estimate.
//! - The secondary track is aligned to the leader by an offset taken from the two live edges. The
//!   offset also covers what the leader still has to slew, so the secondary track goes straight
//!   to its final position instead of following the leader there.
//! - The offset slews faster than the leader's anchor. A correction that presents video earlier
//!   applies at once, one that presents it later waits for a tolerance.
//! - A track that starts once the timelines have converged joins the leader's anchor right away;
//!   tracks that converge later are merged by the corrections.
//!
//! Going back to waiting for a start:
//! - On a pts discontinuity only the track that jumped starts over, and it tells its sink. The
//!   other track keeps playing and leads on its own until the timelines converge again.
//! - A track that stops delivering for long enough to run out of released content starts over
//!   too, so it has to earn its place on the shared timeline again.
//!
//! Decisions (`start_track_decision`, `correct_mode_decision`) read the state and the state
//! applies them.
//!
//! Known issues:
//! - If the input stream clock drifts faster than the anchor slew rate (3% when shrinking the
//!   buffer, 4% when growing it), the correction logic will not keep up. Only drift below that
//!   rate or an immediate timestamp discontinuity larger than 10s is handled.
//! - While the tracks count as neither converged nor diverged (the distance between their live
//!   edges, or between their slowest deliveries, sits between the merge (3s) and the split (5s)
//!   threshold), the secondary track offset is not re-aligned. If the leader was slewing when the
//!   re-alignment stopped, the offset keeps what the leader had left to slew at that moment.
//! - A pts jump below the discontinuity threshold (10s) that is not matched by a gap in arrival
//!   is taken at face value. Forward, the output freezes for the length of the jump and the extra
//!   buffer is slewed away only if it exceeds the maximum. Backward, chunks are late until the
//!   stale estimate resets the track.
//! - Old content delivered while the upper bound is stable (e.g. a backlog flushed by a track
//!   that joins late) counts as slow delivery and inflates the spread until it leaves the window.
//! - Live edges are estimated from pts, so with unrelated timelines the offset between the tracks
//!   is biased by the video reorder depth (B-frames).
//!
//! Live edge detection itself is implemented by `LiveEdgeEstimator`, usable on its own by
//! inputs with different buffering logic.

use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

mod buffer;
mod decision_correct_mode;
mod decision_start_track;
mod edge_estimator;
mod mode;
mod state;
mod stats;
mod track;

pub(crate) use buffer::{BufferingStrategy, ChunkBuffer, FifoBuffer, LiveSyncBuffer};
pub(crate) use track::LiveSyncTrack;

use crate::pipeline::utils::input_sync::{BoxedTrackSink, InputSyncStatsSender, TrackKind};
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
    /// Distance between the upper bound and its recent counterpart above which the estimate of a
    /// started track counts as stale and the track starts over: its timeline slipped and the full
    /// window still holds the old edge.
    pub stale_estimate_threshold: Duration,
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
    pub fn new(options: LiveSyncOptions, sync_point: Instant, stats: InputSyncStatsSender) -> Self {
        let shared = Arc::new(Mutex::new(SharedState::new(options, sync_point, stats)));
        spawn_tick_thread(Arc::downgrade(&shared));
        Self { shared }
    }

    /// Registers the track of the given kind; `sink` receives its chunks
    /// once they are synchronized. Tracks share the live edge detection but
    /// each starts on its own.
    pub fn add_track(&self, kind: TrackKind, sink: BoxedTrackSink<B::Chunk>) -> LiveSyncTrack<B> {
        self.shared.lock().unwrap().add_track(kind, sink);
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
    let span = tracing::Span::current();
    std::thread::Builder::new()
        .name("Live sync ticker".to_string())
        .spawn(move || {
            let _guard = span.entered();
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
