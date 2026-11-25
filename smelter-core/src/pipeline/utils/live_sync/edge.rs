use std::{collections::VecDeque, time::Duration};

use super::{LiveSyncTuning, lateness::HISTORY};
use crate::Timestamp;

/// A chunk this far under the edge moves it only together with the next two (ED2).
const BIG_FALL: Duration = Duration::from_secs(1);

/// Longest window a rise is confirmed over (ED3).
const WINDOW_MAX: Duration = Duration::from_secs(60);

/// A rise moves what was measured before the window, with this margin.
const RISE_MARGIN: Duration = Duration::from_millis(50);

/// The edge moved up by `delta`. What was measured before `since` moves with it (LT7).
pub(super) struct Rise {
    pub delta: Timestamp,
    pub since: Timestamp,
}

/// A rise that can still be taken back.
struct PastRise {
    delta: Timestamp,
    since: Timestamp,
    edge_before: Timestamp,
    /// Arrival of the last chunk at which the edge was still above half of the rise.
    seen_high: Timestamp,
}

/// Fastest delivery of a track: the smallest delivery delay (`arrival - pts`). Falls at once,
/// rises only when a whole window is later and delivery is not catching up.
pub(super) struct Edge {
    settle: Duration,
    tolerance: Duration,
    window_base: Duration,
    value: Timestamp,
    /// `(arrival, delay)` of the chunks inside the window.
    samples: VecDeque<(Timestamp, Timestamp)>,
    /// Arrivals of the last four chunks.
    last_arrivals: VecDeque<Timestamp>,
    /// How long regular lateness lasts (LT9), set by the caller.
    away: Duration,
    /// ED4: level the edge has to fall under to count as still falling, and when it last did.
    mark: Timestamp,
    last_fall: Timestamp,
    /// ED2: chunks in a row that are more than `BIG_FALL` under the edge.
    pending: Vec<(Timestamp, Timestamp)>,
    /// Rises that were not taken back, oldest first.
    risen: Vec<PastRise>,
}

impl Edge {
    /// ED1: the edge starts as the delivery delay of the first chunk.
    pub fn new(tuning: &LiveSyncTuning, arrival: Timestamp, delay: Timestamp) -> Self {
        Self {
            settle: tuning.settle,
            tolerance: tuning.tolerance,
            window_base: tuning.window_base,
            value: delay,
            samples: VecDeque::from([(arrival, delay)]),
            last_arrivals: VecDeque::from([arrival]),
            away: Duration::ZERO,
            mark: delay,
            last_fall: arrival,
            pending: Vec::new(),
            risen: Vec::new(),
        }
    }

    pub fn value(&self) -> Timestamp {
        self.value
    }

    pub fn last_arrival(&self) -> Timestamp {
        self.last_arrivals[self.last_arrivals.len() - 1]
    }

    pub fn set_away(&mut self, away: Duration) {
        self.away = away;
    }

    /// W: twice as long as regular lateness lasts.
    fn base_window(&self) -> Duration {
        Duration::clamp(self.away * 2, self.window_base, WINDOW_MAX)
    }

    /// W, or back to the fourth chunk from the end on a track that delivers less.
    fn window(&self, now: Timestamp) -> Duration {
        let mut window = self.base_window();
        if self.last_arrivals.len() == 4 {
            let since_fourth = (now - self.last_arrivals[0]).to_duration_saturating();
            window = Duration::max(window, since_fourth);
        }
        Duration::min(window, WINDOW_MAX)
    }

    /// ED2. Returns the rises this chunk took back.
    pub fn observe(&mut self, arrival: Timestamp, delay: Timestamp) -> Vec<Rise> {
        if self.last_arrivals.len() == 4 {
            self.last_arrivals.pop_front();
        }
        self.last_arrivals.push_back(arrival);

        if delay < self.value - BIG_FALL {
            // one broken timestamp must not move anything
            self.pending.push((arrival, delay));
            if self.pending.len() < 3 {
                return Vec::new();
            }
            self.samples = self.pending.drain(..).collect();
            self.value = self
                .samples
                .iter()
                .map(|(_, delay)| *delay)
                .min()
                .unwrap_or(delay);
        } else {
            self.pending.clear();
            self.samples.push_back((arrival, delay));
            self.value = Timestamp::min(self.value, delay);
        }

        // LT7: a rise is taken back when the edge returns at once to where it was before it
        let (value, tolerance) = (self.value, self.tolerance);
        let window = self.window(arrival);
        let mut taken_back = Vec::new();
        while let Some(rise) = self
            .risen
            .pop_if(|rise| value <= rise.edge_before + tolerance)
        {
            if rise.seen_high + window >= arrival {
                taken_back.push(Rise {
                    delta: rise.delta,
                    since: rise.since,
                });
            }
        }
        for rise in &mut self.risen {
            if value >= rise.edge_before + rise.delta / 2i64 {
                rise.seen_high = arrival;
            }
        }

        if value < self.mark - tolerance {
            self.mark = value;
            self.last_fall = arrival;
        }
        taken_back
    }

    /// `c`: the smallest delivery delay among the chunks of the last W.
    pub fn candidate(&self, now: Timestamp) -> Option<Timestamp> {
        let since = now - self.base_window();
        self.samples
            .iter()
            .filter(|(arrival, _)| *arrival >= since)
            .map(|(_, delay)| *delay)
            .min()
    }

    /// ED3: the edge rises when both halves of the window are away from it and the later one is
    /// not faster.
    pub fn tick(&mut self, now: Timestamp) -> Option<Rise> {
        // its undo would shift nothing in the lateness history by then
        self.risen.retain(|rise| rise.since + HISTORY > now);

        let window = self.window(now);
        while let Some((arrival, _)) = self.samples.front()
            && *arrival < now - window
        {
            self.samples.pop_front();
        }

        let half = now - window / 2;
        let fastest = |later_half: bool| {
            self.samples
                .iter()
                .filter(|(arrival, _)| (*arrival >= half) == later_half)
                .map(|(_, delay)| *delay)
                .min()
        };
        let (Some(earlier), Some(later)) = (fastest(false), fastest(true)) else {
            // time without arrivals proves nothing
            return None;
        };
        let candidate = Timestamp::min(earlier, later);
        let catching_up = later < earlier - self.tolerance;
        if candidate <= self.value + self.tolerance || catching_up {
            return None;
        }

        let rise = Rise {
            delta: candidate - self.value,
            since: now - window - RISE_MARGIN,
        };
        self.risen.push(PastRise {
            delta: rise.delta,
            since: rise.since,
            edge_before: self.value,
            seen_high: now,
        });
        self.value = candidate;
        self.mark = candidate;
        Some(rise)
    }

    /// ED4: the edge has not gone down for `settle`, so a burst has ended.
    pub fn settled(&self, now: Timestamp) -> bool {
        self.last_fall + self.settle <= now
    }
}
