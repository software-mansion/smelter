use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::{
    LiveSyncOptions,
    buffer::LiveSyncBuffer,
    edge_estimator::LiveEdgeEstimator,
    flush::{FlushQueue, TrackFlushState},
    state::{EdgeSource, SharedState},
};
use crate::{
    pipeline::utils::input_sync::{InputSyncItem, TimestampAnchor},
    utils::live_sync::{buffer::BufferCheckResult, state::resolve_should_start},
};

/// pts jump (in either direction) treated as a discontinuity of the input
/// timeline; the old edge estimate does not describe the new timeline.
const DISCONTINUITY_THRESHOLD: Duration = Duration::from_secs(10);

/// Lead over the playback position content needs to still reach the queue.
const MIN_QUEUE_HEADROOM: Duration = Duration::from_millis(80);

/// Buffers chunks of a single track until the live edge is detected. Cheap to
/// move to another thread; only feeding the shared estimator and pre-start
/// checks take a short-lived lock on the input's shared state.
pub(crate) struct LiveSyncTrack<B: LiveSyncBuffer> {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,

    shared: Arc<Mutex<SharedState>>,

    state: TrackState,

    /// Estimator observing only this track's chunks.
    estimator: LiveEdgeEstimator,

    // Current buffer
    buffer: B,

    // When we detect discontinuity current state of the buffer still needs
    // to be returned, but we already need to "observe" the packet that caused
    // discontinuity
    flush_queue: FlushQueue<B>,
    flush_state: TrackFlushState,
}

impl<B: LiveSyncBuffer> LiveSyncTrack<B> {
    pub(super) fn new(
        options: LiveSyncOptions,
        sync_point: Instant,
        shared: Arc<Mutex<SharedState>>,
        flush_state: TrackFlushState,
    ) -> Self {
        Self {
            estimator: LiveEdgeEstimator::new(sync_point, options.stabilization_tolerance),
            options,
            sync_point,
            shared,
            buffer: B::default(),
            flush_queue: FlushQueue::default(),
            flush_state,
            state: TrackState::WaitingForStart,
        }
    }

    pub fn write_chunk(&mut self, item: B::Item) {
        if self.flush_state.should_flush() {
            self.reset();
        }

        let now = Instant::now();
        self.check_discontinuity(now, item.pts());

        {
            // both estimators observe for the whole lifetime of the input
            self.estimator.observe(now, item.pts());
            let mut shared = self.shared.lock().unwrap();
            shared.shared_estimator.observe(now, item.pts());
            self.buffer.write(item);
        }

        self.maybe_start();
        self.maybe_correct();
    }

    pub fn try_read_chunk(&mut self) -> Option<B::Item> {
        if self.flush_state.should_flush() {
            self.reset();
        }
        self.maybe_start();
        self.maybe_correct();

        if let Some(item) = self.flush_queue.read() {
            return Some(item);
        }

        // check current buffer
        let TrackState::Started {
            target_anchor,
            ref mut anchor,
            ..
        } = self.state
        else {
            return None;
        };

        let max_pts = self.buffer.max_read_pts();
        let mut chunk = self.buffer.try_read()?;
        let pts_diff = max_pts
            .map(|max| chunk.pts().saturating_sub(max))
            .unwrap_or_default();
        let max_shift = self
            .options
            .buffering_strategy
            .max_shift(*anchor, target_anchor, pts_diff);
        anchor.nudge_towards(target_anchor, max_shift);
        chunk.apply_anchor(*anchor);
        Some(chunk)
    }

    /// Output pts of the next readable chunk; `None` while the live edge is
    /// still being detected or when nothing is buffered. Enables interleaved
    /// reads across tracks.
    pub fn peek_next_pts(&mut self) -> Option<Duration> {
        if self.flush_state.should_flush() {
            self.reset();
        }

        self.maybe_start();
        self.maybe_correct();

        if let Some(pts) = self.flush_queue.peek_pts() {
            return Some(pts);
        }

        // check current buffer
        let anchor = self.state.anchor()?; // TODO: it should return even if no anchor (the same behavior as flush)
        let chunk = self.buffer.peek()?;
        Some(anchor.to_output_pts(chunk.pts()))
    }

    /// Runs the start decision; called without new chunks too, so time-based
    /// conditions can trigger the start when delivery pauses.
    fn maybe_start(&mut self) {
        if !matches!(self.state, TrackState::WaitingForStart) {
            return;
        }
        let now = Instant::now();
        let mut shared = self.shared.lock().unwrap();

        let estimator = resolve_should_start(
            now,
            &self.options,
            &self.estimator,
            &shared.shared_estimator,
        );
        let Some(estimator) = estimator else {
            return;
        };

        let estimation = match estimator {
            EdgeSource::Shared if let Some(anchor) = shared.anchor => {
                self.state = TrackState::Started {
                    anchor,
                    target_anchor: anchor,
                    edge_source: EdgeSource::Shared,
                };
                return;
            }
            EdgeSource::Shared => shared.shared_estimator.estimate(now),
            EdgeSource::Track => self.estimator.estimate(now),
        };
        let Some(estimation) = estimation else {
            return;
        };

        let now_pts = now.saturating_duration_since(self.sync_point);
        let anchor = TimestampAnchor {
            input_pts: estimation.upper_bound.pts,
            output_pts: now_pts + self.options.buffering_strategy.desired_buffer(),
        };

        if estimator == EdgeSource::Shared {
            shared.anchor = Some(anchor);
        }
        self.state = TrackState::Started {
            target_anchor: anchor,
            anchor,
            edge_source: estimator,
        };
    }

    fn maybe_correct(&mut self) {
        let TrackState::Started {
            target_anchor,
            anchor,
            edge_source,
        } = &mut self.state
        else {
            return;
        };

        let now = Instant::now();
        let now_pts = now.saturating_duration_since(self.sync_point);
        let mut shared = self.shared.lock().unwrap();

        match edge_source {
            EdgeSource::Shared => {
                let Some(estimation) = shared.shared_estimator.estimate(now) else {
                    return;
                };

                let Some(current_anchor) = shared.anchor else {
                    return;
                };
                let result = self.options.buffering_strategy.check(
                    estimation,
                    current_anchor,
                    self.sync_point,
                );
                match result {
                    BufferCheckResult::Ok => (),
                    BufferCheckResult::TooSmall | BufferCheckResult::TooLarge => {
                        shared.anchor = Some(TimestampAnchor {
                            input_pts: estimation.upper_bound.pts,
                            output_pts: now_pts + self.options.buffering_strategy.desired_buffer(),
                        })
                    }
                }
                if let Some(shared_anchor) = shared.anchor {
                    *target_anchor = shared_anchor
                }
            }
            EdgeSource::Track => {
                let Some(estimation) = self.estimator.estimate(now) else {
                    return;
                };

                let result =
                    self.options
                        .buffering_strategy
                        .check(estimation, *anchor, self.sync_point);

                match result {
                    BufferCheckResult::Ok => (),
                    BufferCheckResult::TooSmall | BufferCheckResult::TooLarge => {
                        *target_anchor = TimestampAnchor {
                            input_pts: estimation.upper_bound.pts,
                            output_pts: now_pts + self.options.buffering_strategy.desired_buffer(),
                        }
                    }
                }
            }
        }
    }

    fn best_effort_anchor(&self, now: Instant) -> Option<TimestampAnchor> {
        let now_pts = now.saturating_duration_since(self.sync_point);
        // Continue where the flushed content ends, unless it ends too close
        // to the playback position to still reach the queue.
        let output_pts = match self.flush_queue.end_pts() {
            Some(end_pts) if end_pts > now_pts + MIN_QUEUE_HEADROOM => end_pts,
            _ => now_pts + self.options.buffering_strategy.desired_buffer(),
        };
        Some(TimestampAnchor {
            input_pts: self.buffer.peek()?.pts(),
            output_pts,
        })
    }

    fn check_discontinuity(&mut self, now: Instant, pts: Duration) {
        let Some(estimation) = self.estimator.estimate(now) else {
            return;
        };
        let delivery = estimation.delivery;
        // pts expected if the stream kept producing in real time since the
        // newest received chunk
        let expected_pts = delivery.last_pts + delivery.since_last_arrival;
        let forward_jump = pts > expected_pts + DISCONTINUITY_THRESHOLD;
        let backward_jump = pts + DISCONTINUITY_THRESHOLD < delivery.last_pts;
        if forward_jump || backward_jump {
            self.flush_state.flush();
            // Only the detecting track resets shared state. Followers can't
            // reach this branch: their reset clears the estimator `estimate`
            // needs above.
            {
                let mut shared = self.shared.lock().unwrap();
                shared.shared_estimator =
                    LiveEdgeEstimator::new(self.sync_point, self.options.stabilization_tolerance);
                shared.anchor = None;
            }
            self.reset();
        }
    }

    fn reset(&mut self) {
        let now = Instant::now();
        // the estimator only observed this timeline, so its newest pts covers
        // everything the flushed buffer can release
        let last_pts = self.estimator.estimate(now).map(|e| e.delivery.last_pts);
        let anchor = self.state.anchor().or_else(|| self.best_effort_anchor(now));

        self.estimator =
            LiveEdgeEstimator::new(self.sync_point, self.options.stabilization_tolerance);
        self.state = TrackState::WaitingForStart;

        if let Some(anchor) = anchor {
            let buffer = std::mem::take(&mut self.buffer);
            self.flush_queue.push(buffer, anchor, last_pts);
        };
    }
}

enum TrackState {
    /// Written chunks are buffered and never returned. On each write
    /// we are checking if both edge estimators are ready.
    ///
    /// If shared and track estimator diverge by more than
    /// `shared_edge_tolerance` switch to `StartedWithTrackEstimator`,
    /// otherwise switch to `StartedWithSharedEstimator`.
    WaitingForStart,
    Started {
        target_anchor: TimestampAnchor,
        anchor: TimestampAnchor,
        edge_source: EdgeSource,
    },
}

impl TrackState {
    fn anchor(&self) -> Option<TimestampAnchor> {
        match self {
            TrackState::WaitingForStart => None,
            TrackState::Started { anchor, .. } => Some(*anchor),
        }
    }
}
