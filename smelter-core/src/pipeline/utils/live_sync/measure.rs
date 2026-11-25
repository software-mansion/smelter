use std::time::Duration;

use tracing::debug;

use super::{LiveSyncOptions, edge::Edge, lateness::Lateness};
use crate::Timestamp;

/// TR1: a track that delivered nothing for this long stops counting.
const IDLE: Duration = Duration::from_secs(10);

/// Stage 1 for one track, from its first chunk on: where its live edge is and how late its
/// delivery gets regularly. Sees only arrival times and pts.
pub(super) struct Measure {
    edge: Edge,
    lateness: Lateness,
    /// TR1: the track is part of what the input measures. Its first burst has ended.
    counts: bool,
    first_arrival: Timestamp,
    start_timeout: Duration,
}

impl Measure {
    pub fn new(options: &LiveSyncOptions, arrival: Timestamp, delay: Timestamp) -> Self {
        Self {
            edge: Edge::new(&options.tuning, arrival, delay),
            lateness: Lateness::new(options.tuning.tolerance, options.segmented),
            counts: false,
            first_arrival: arrival,
            start_timeout: options.tuning.start_timeout,
        }
    }

    /// Returns how much of earlier rises of the edge this chunk took back (LT7).
    pub fn observe(&mut self, arrival: Timestamp, delay: Timestamp) -> Timestamp {
        let gap = (arrival - self.edge.last_arrival()).to_duration_saturating();
        let mut taken_back = Timestamp::ZERO;
        for rise in self.edge.observe(arrival, delay) {
            debug!(delta=?rise.delta, "Live sync: rise of the edge taken back, it was lateness");
            self.lateness.shift(-rise.delta, rise.since);
            taken_back += rise.delta;
        }
        // LT8
        if self.counts {
            self.lateness.observe(arrival, delay, gap);
        }
        taken_back
    }

    /// `started`: the input has a playback timeline (DC1).
    pub fn tick(&mut self, now: Timestamp, started: bool) {
        if self.counts {
            let (_, lasts) = self.lateness.regular(now, self.edge.value());
            self.edge.set_away(lasts);
        }
        if let Some(rise) = self.edge.tick(now) {
            debug!(delta=?rise.delta, edge=?self.edge.value(), "Live sync: edge rose");
            self.lateness.shift(rise.delta, rise.since);
        }
        let timed_out = self.first_arrival + self.start_timeout <= now;
        if !self.counts && started && (self.edge.settled(now) || timed_out) {
            self.counts = true;
        }
    }

    pub fn edge(&self) -> Timestamp {
        self.edge.value()
    }

    /// `L`: how far above the edge the delivery delay gets regularly.
    pub fn regular_lateness(&mut self, now: Timestamp) -> Timestamp {
        self.lateness.regular(now, self.edge.value()).0
    }

    /// `c`: where delivery is now. The edge when nothing arrived in the last W.
    pub fn candidate(&self, now: Timestamp) -> Timestamp {
        self.edge.candidate(now).unwrap_or(self.edge.value())
    }

    pub fn settled(&self, now: Timestamp) -> bool {
        self.edge.settled(now)
    }

    pub fn counts(&self) -> bool {
        self.counts
    }

    /// DC1: at the start every track that has delivered counts.
    pub fn start_counting(&mut self) {
        self.counts = true;
    }

    pub fn first_arrival(&self) -> Timestamp {
        self.first_arrival
    }

    pub fn is_idle(&self, now: Timestamp) -> bool {
        self.edge.last_arrival() + IDLE < now
    }
}
