//! Live synchronization for live inputs (RTMP, HLS, MoQ): decides when each chunk of an input
//! is presented, so that playback stays a chosen time behind live.
//!
//! What is measured, per track:
//! - delivery delay of a chunk: `arrival - pts`. Only differences matter.
//! - live edge: the delivery delay of the fastest delivery. Content can't arrive before it
//!   exists, so that is where live is.
//! - regular lateness: how far above the edge the delivery delay gets regularly (jitter, an HLS
//!   segment, a stall that keeps coming back). One late chunk or one burst is not regular.
//!
//! What is controlled: the playback timeline `T`. A chunk is presented at `pts + T`, so the
//! buffer of the fastest packets is `T - edge`. It starts at `start`, and it is left alone while
//! the regular slowest packets have at least `lower` and the fastest ones at most `upper`
//! ([`LiveSyncOptions`]). Outside of that band the timeline is re-anchored to the target, by
//! a slew when possible and by a jump when content is already being lost or nothing is
//! playing anyway.
//!
//! Four stages, run on every written chunk and on a periodic tick:
//!
//! | stage | file | owns |
//! |---|---|---|
//! | 0 stitch | `stitch.rs` | the pts offset that hides pts discontinuities |
//! | 1 measure | `edge.rs`, `lateness.rs`, `measure.rs` | edge and regular lateness of a track |
//! | 2 decide | `decide.rs`, `timeline.rs` | the playback timeline |
//! | 3 release | `release.rs` | the buffer of a track: what is released, kept or dropped |
//!
//! Stage 1 does not know what is being played and stage 2 does not see single chunks, so a
//! late chunk can't move the timeline directly.
//!
//! The rules are specified in `DESIGN.md` next to this file. Comments refer to them by their
//! ids (`ED1`, `LT3`, `DC7`, ...). `design_sim.py` is a model of stages 1 and 2 that `edge.rs`,
//! `lateness.rs` and `decide.rs` follow.

use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

mod buffer;
mod decide;
mod edge;
mod lateness;
mod measure;
mod release;
mod state;
mod stats;
mod stitch;
mod timeline;
mod track;

pub(crate) use buffer::{FifoBuffer, LiveSyncBuffer};
pub(crate) use track::{LiveSyncDeadline, LiveSyncTrack};

use crate::pipeline::utils::input_sync::{BoxedTrackSink, InputSyncStatsSender, TrackKind};
use state::SharedState;

/// RL5: the most content a track holds. Beyond it the oldest content is dropped.
pub(crate) const CAPACITY: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncOptions {
    /// The least buffer the regular slowest packets should have.
    pub lower: Duration,
    /// The starting buffer of the fastest packets, and where the buffer is brought back to.
    pub start: Duration,
    /// The most buffer the fastest packets should have.
    pub upper: Duration,
    pub at_upper: AtUpperLimit,
    /// Hint: content is delivered in segments, so the first chunk after a pause in delivery
    /// shows how late delivery gets.
    pub segmented: bool,
    /// Hint: the tracks share one timeline. Without it, tracks whose live edges are far apart
    /// are synchronized separately.
    pub shared_timeline: bool,
    pub tuning: LiveSyncTuning,
}

/// What happens when `lower` and `upper` can't both hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AtUpperLimit {
    /// `lower` wins: the buffer grows past `upper` as far as the regular lateness needs.
    Extend,
    /// `upper` wins: playback is never more than `upper` behind the live edge of the reference
    /// track (audio while it delivers). What can't arrive in that time is late.
    Enforce,
}

/// Per-protocol constants.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveSyncTuning {
    /// How long the live edge has to stay where it is to be settled: the burst that a stream
    /// starts with has ended.
    pub settle: Duration,
    /// Differences smaller than this are not acted on.
    pub tolerance: Duration,
    /// The shortest time slower delivery has to last before the live edge follows it.
    pub window_base: Duration,
    /// Playback starts this long after the first chunk even when the live edge never settles.
    pub start_timeout: Duration,
    /// The largest buffer excess that is removed by a slew; more than that is a jump.
    pub max_slew: Duration,
    /// Whether pts discontinuities are stitched. Only for buffers that keep the write order.
    pub stitch: bool,
}

/// Synchronization of a single input; create per-track handles with
/// [`LiveSync::add_track`]. The buffer type decides the order chunks of a track leave in
/// (e.g. [`FifoBuffer`] for in-order delivery).
pub(crate) struct LiveSync<B: LiveSyncBuffer> {
    shared: Arc<Mutex<SharedState<B>>>,
}

/// How often the internal ticker thread runs the stages while nothing is written.
const TICK_INTERVAL: Duration = Duration::from_millis(20);

impl<B: LiveSyncBuffer> LiveSync<B> {
    pub fn new(options: LiveSyncOptions, sync_point: Instant, stats: InputSyncStatsSender) -> Self {
        let shared = Arc::new(Mutex::new(SharedState::new(options, sync_point, stats)));
        spawn_tick_thread(Arc::downgrade(&shared));
        Self { shared }
    }

    /// Registers the track of the given kind; `sink` receives its chunks
    /// once they are synchronized.
    pub fn add_track(&self, kind: TrackKind, sink: BoxedTrackSink<B::Chunk>) -> LiveSyncTrack<B> {
        let deadline = self.shared.lock().unwrap().add_track(kind, sink);
        LiveSyncTrack::new(self.shared.clone(), kind, deadline)
    }

    /// Releases everything the track buffered and removes it, dropping its sink. A track of the
    /// same kind can be added again afterwards.
    pub fn remove_track(&self, kind: TrackKind) {
        self.shared.lock().unwrap().remove_track(kind);
    }

    /// Every track releases everything it buffered (e.g. when the stream ended).
    pub fn flush(&self) {
        self.shared.lock().unwrap().flush();
    }

    /// Changes the hint [`LiveSyncOptions::shared_timeline`], for inputs that learn it while
    /// they run.
    pub fn set_shared_timeline(&self, shared_timeline: bool) {
        self.shared.lock().unwrap().options.shared_timeline = shared_timeline;
    }
}

/// Runs the stages while delivery pauses, pushing releasable chunks to the track sinks; exits
/// once every handle to the input is dropped.
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
                shared.lock().unwrap().tick();
            }
        })
        .unwrap();
}
