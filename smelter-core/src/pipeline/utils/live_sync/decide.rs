use std::time::Duration;

use tracing::debug;

use super::{AtUpperLimit, LiveSyncOptions, timeline::Timeline};
use crate::{Timestamp, pipeline::utils::input_sync::TrackKind};

/// DC4: slew rate, per second of content.
const RATE_MIN: f64 = 0.01;
const RATE_MAX: f64 = 0.04;

/// DC4: the rate is chosen so that a slew takes about this long.
const RATE_TIME: Duration = Duration::from_secs(5);

/// A re-anchor this close to its target is over.
pub(super) const REACHED: Timestamp = Timestamp::from_millis(2);

/// What stage 2 reads from a track that has delivered something.
#[derive(Debug, Clone, Copy)]
pub(super) struct TrackView {
    pub kind: TrackKind,
    pub edge: Timestamp,
    /// `L`
    pub lateness: Timestamp,
    /// `c`
    pub candidate: Timestamp,
    /// TR1
    pub counts: bool,
    pub idle: bool,
    /// DC6: content it released is still to be presented.
    pub playing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Later,
    Earlier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReanchorKind {
    Jump,
    Slew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReanchorReason {
    BufferTooSmall,
    BufferTooLarge,
    /// Regular packets already miss their time.
    ContentLate,
    /// Nothing was playing, so a jump costs nothing.
    Interrupted,
    /// DC7
    AddedLatencyTakenBack,
    /// TR6: the timeline of video moves to the one of audio.
    TracksJoined,
}

/// A re-anchor that stage 2 started.
#[derive(Debug, Clone, Copy)]
pub(super) struct Reanchor {
    pub kind: ReanchorKind,
    pub reason: ReanchorReason,
}

/// What stage 2 measured and aimed for on its last run.
#[derive(Debug, Clone, Copy)]
pub(super) struct Status {
    /// `E`
    pub edge: Timestamp,
    /// `L`
    pub lateness: Timestamp,
    /// `T*`
    pub target: Timestamp,
    /// The band, as values of `T`.
    pub lowest: Timestamp,
    pub highest: Timestamp,
    /// TR5
    pub reference: TrackKind,
}

/// One playback timeline and what stage 2 remembers about it.
#[derive(Debug, Clone)]
pub(super) struct Playback {
    pub timeline: Timeline,
    /// DC7: the timeline without the latency that re-anchors added here.
    baseline: Timestamp,
    /// DC7: how much of the rises of the edge the timeline has followed, and has not yet.
    followed: Timestamp,
    unfollowed: Timestamp,
    /// `E` and `T` on the last run.
    last_edge: Option<Timestamp>,
    last_value: Timestamp,
    /// DC3: a re-anchor by a slew is running.
    moving: Option<Direction>,
    /// DC7: the slew that takes added latency back is running.
    returning: bool,
    /// TR6: the slew to the timeline of the other track is running.
    joining: bool,
    /// RL4, RL7: while nothing plays, the `T` that a restart at where delivery is now would
    /// give, when that is later than `T`.
    pub restart: Option<Timestamp>,
    pub status: Option<Status>,
}

impl Playback {
    /// DC1
    pub fn start(value: Timestamp) -> Self {
        Self {
            timeline: Timeline::new(value),
            baseline: value,
            followed: Timestamp::ZERO,
            unfollowed: Timestamp::ZERO,
            last_edge: None,
            last_value: value,
            moving: None,
            returning: false,
            joining: false,
            restart: None,
            status: None,
        }
    }

    /// A slew is running.
    pub fn is_reanchoring(&self) -> bool {
        self.moving.is_some() || self.returning || self.joining
    }

    /// DC7: a rise of the edge was taken back (LT7), so the re-anchor that followed it added
    /// latency after all.
    pub fn rise_taken_back(&mut self, delta: Timestamp) {
        let unfollowed = Timestamp::min(delta, self.unfollowed);
        let followed = Timestamp::min(delta - unfollowed, self.followed);
        self.baseline -= followed;
        self.followed -= followed;
        self.unfollowed -= unfollowed;
    }

    /// DC7: a move to later adds latency, except where it follows a rise of the edge.
    fn account(&mut self, value: Timestamp) {
        if value > self.last_value {
            let follows = Timestamp::min(value - self.last_value, self.unfollowed);
            self.baseline += follows;
            self.followed += follows;
            self.unfollowed -= follows;
        }
        self.baseline = Timestamp::min(self.baseline, value);
        self.last_value = value;
    }

    /// Stage 2 for the tracks on this timeline: combines what they measured into the target and
    /// the band, and re-anchors the timeline when it is outside (DC2, DC3) or holds latency
    /// that was added here (DC7).
    pub fn decide(
        &mut self,
        options: &LiveSyncOptions,
        tracks: [Option<TrackView>; 2],
    ) -> Option<Reanchor> {
        let tolerance = options.tuning.tolerance;
        let tracks = || tracks.iter().flatten();
        let interrupted = tracks().all(|track| !track.playing);
        self.joining = false;

        // TR1, TR4: an idle track stops counting, unless every track is idle
        let any_fresh = tracks().any(|track| track.counts && !track.idle);
        let active = || tracks().filter(move |track| track.counts && (!track.idle || !any_fresh));
        // TR5
        let reference = active()
            .find(|track| track.kind == TrackKind::Audio)
            .or_else(|| active().next());
        let Some(reference) = reference else {
            self.restart = None;
            return None;
        };

        // 2.1, 2.4
        let edge = active().fold(reference.edge, |edge, track| {
            Timestamp::max(edge, track.edge)
        });
        let slowest = active().fold(reference.edge, |slowest, track| {
            Timestamp::max(slowest, track.edge + track.lateness)
        });
        let candidate = active().fold(reference.candidate, |candidate, track| {
            Timestamp::max(candidate, track.candidate)
        });
        let lateness = slowest - edge;
        let buffer = Timestamp::max(options.start.into(), lateness + options.lower);
        let mut target = edge + buffer;
        let mut lowest = slowest + options.lower;
        let mut highest = edge + options.upper + (buffer - options.start);
        let mut restart = candidate + buffer;
        // regular packets that must not be lost: a jump is worth it for them
        let mut must_reach = slowest;
        if options.at_upper == AtUpperLimit::Enforce {
            let cap = reference.edge + options.upper;
            target = Timestamp::min(target, cap);
            lowest = Timestamp::min(lowest, cap);
            highest = Timestamp::min(highest, cap);
            restart = Timestamp::min(restart, reference.candidate + options.upper);
            must_reach = reference.edge + reference.lateness;
        }

        // DC7: what a slew moved since the last run, then a rise of the edge. The rise first
        // turns latency that was added into followed: the stream moved to where the timeline
        // already was.
        let value = self.timeline.value();
        self.account(value);
        if let Some(last_edge) = self.last_edge
            && edge > last_edge
        {
            let rise = edge - last_edge;
            let follows = Timestamp::min(rise, value - self.baseline);
            self.baseline += follows;
            self.followed += follows;
            self.unfollowed += rise - follows;
        }
        self.last_edge = Some(edge);

        let error = target - value;
        let outside = value < lowest - tolerance || value > highest + tolerance;
        let reached = match self.moving {
            Some(Direction::Later) => error <= REACHED,
            Some(Direction::Earlier) => error >= -REACHED,
            None => false,
        };
        if reached {
            // or the target is on the other side by now
            self.moving = None;
        }
        // added here and above the target
        let added = Timestamp::min(value - self.baseline, -error);

        let mut reanchor = None;
        if interrupted {
            // a slew to later becomes a jump, one to earlier waits
            if outside || self.moving == Some(Direction::Later) {
                reanchor = Some(Reanchor {
                    kind: ReanchorKind::Jump,
                    reason: ReanchorReason::Interrupted,
                });
            } else {
                self.timeline.slew(0.0, value);
            }
        } else if self.moving.is_some() || outside {
            let late = error.is_positive() && value < must_reach - tolerance;
            if late {
                reanchor = Some(Reanchor {
                    kind: ReanchorKind::Jump,
                    reason: ReanchorReason::ContentLate,
                });
            } else if value - highest > Timestamp::from(options.tuning.max_slew) {
                reanchor = Some(Reanchor {
                    kind: ReanchorKind::Jump,
                    reason: ReanchorReason::BufferTooLarge,
                });
            } else {
                if self.moving.is_none() {
                    reanchor = Some(Reanchor {
                        kind: ReanchorKind::Slew,
                        reason: match error.is_positive() {
                            true => ReanchorReason::BufferTooSmall,
                            false => ReanchorReason::BufferTooLarge,
                        },
                    });
                }
                self.moving = Some(match error.is_positive() {
                    true => Direction::Later,
                    false => Direction::Earlier,
                });
                self.timeline.slew(slew_rate(error), target);
            }
            self.returning = false;
        } else if self.returning || added > Timestamp::from(tolerance) {
            if !self.returning {
                reanchor = Some(Reanchor {
                    kind: ReanchorKind::Slew,
                    reason: ReanchorReason::AddedLatencyTakenBack,
                });
            }
            self.returning = added > REACHED;
            match self.returning {
                true => self.timeline.slew(slew_rate(-added), value - added),
                false => self.timeline.slew(0.0, value),
            }
        } else {
            self.timeline.slew(0.0, value);
        }

        if let Some(Reanchor {
            kind: ReanchorKind::Jump,
            ..
        }) = reanchor
        {
            self.timeline.jump(target);
            self.moving = None;
            self.account(target);
        }
        if let Some(reanchor) = reanchor {
            debug!(
                ?reanchor,
                change=?error,
                buffer=?(value - edge),
                target_buffer=?(target - edge),
                band=?(lowest - edge, highest - edge),
                regular_lateness=?lateness,
                added_latency=?(value - self.baseline),
                reference=?reference.kind,
                "Live sync: re-anchor"
            );
        }

        let value = self.timeline.value();
        self.restart = (interrupted && restart > value + tolerance).then_some(restart);
        self.status = Some(Status {
            edge,
            lateness,
            target,
            lowest,
            highest,
            reference: reference.kind,
        });
        reanchor
    }

    /// TR6: slews toward the timeline the tracks are joined on.
    pub fn join(&mut self, target: Timestamp) -> Option<Reanchor> {
        let error = target - self.timeline.value();
        self.timeline.slew(slew_rate(error), target);
        self.restart = None;
        let started = !self.joining;
        self.joining = true;
        started.then_some(Reanchor {
            kind: ReanchorKind::Slew,
            reason: ReanchorReason::TracksJoined,
        })
    }
}

/// DC4, signed like `error`.
fn slew_rate(error: Timestamp) -> f64 {
    let rate = error.abs_duration().as_secs_f64() / RATE_TIME.as_secs_f64();
    let rate = f64::clamp(rate, RATE_MIN, RATE_MAX);
    match error.is_negative() {
        true => -rate,
        false => rate,
    }
}
