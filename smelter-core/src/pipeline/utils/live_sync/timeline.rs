use std::{collections::VecDeque, time::Duration};

use crate::Timestamp;

/// Content this far behind the newest released pts still reads the `T` it had when the content
/// around it was released (frame reordering, a track that is behind). Content further behind
/// does not continue what was released (a pts discontinuity that was not stitched).
const HISTORY: Duration = Duration::from_secs(5);

/// The playback timeline `T` as a function of the pts (DC5): content is presented at
/// `pts + T(pts)`.
///
/// A re-anchor changes `T` only for content after the newest pts that was already released (the
/// frontier), so equal pts on two tracks are always presented at the same time.
#[derive(Debug, Clone)]
pub(super) struct Timeline {
    /// Oldest first; each one holds for the pts after its `from`, up to the next one.
    segments: VecDeque<Segment>,
    /// Newest pts that was released.
    frontier: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    /// `None`: nothing was released before it.
    from: Option<Timestamp>,
    /// `T` at `from`.
    value: Timestamp,
    /// Slew: change of `T` per second of content.
    rate: f64,
    /// Where the slew ends.
    target: Timestamp,
}

impl Segment {
    fn at(&self, pts: Timestamp) -> Timestamp {
        let content = match self.from {
            Some(from) => Timestamp::max(pts - from, Timestamp::ZERO),
            None => Timestamp::ZERO,
        };
        let value = self.value + content.mul_f64(self.rate);
        match self.target >= self.value {
            true => Timestamp::min(value, self.target),
            false => Timestamp::max(value, self.target),
        }
    }
}

impl Timeline {
    pub fn new(value: Timestamp) -> Self {
        Self {
            segments: VecDeque::from([Segment {
                from: None,
                value,
                rate: 0.0,
                target: value,
            }]),
            frontier: None,
        }
    }

    /// `T` of the content that is released next.
    pub fn value(&self) -> Timestamp {
        match (self.segments.back(), self.frontier) {
            (Some(segment), Some(frontier)) => segment.at(frontier),
            (Some(segment), None) => segment.value,
            (None, _) => Timestamp::ZERO,
        }
    }

    pub fn at(&self, pts: Timestamp) -> Timestamp {
        if self.is_new_content(pts) {
            return self.value();
        }
        let segment = self
            .segments
            .iter()
            .rev()
            .find(|segment| segment.from.is_none_or(|from| from < pts))
            .or(self.segments.front());
        match segment {
            Some(segment) => segment.at(pts),
            None => Timestamp::ZERO,
        }
    }

    /// `T` for a chunk that is released now.
    pub fn release(&mut self, pts: Timestamp) -> Timestamp {
        let value = self.at(pts);
        if self.is_new_content(pts) {
            // it starts over with the `T` of now; a slew is set again on the next run
            *self = Self::new(value);
        }
        match self.frontier {
            Some(frontier) => self.frontier = Some(Timestamp::max(frontier, pts)),
            None => {
                // a slew set before anything was released starts with the first content
                if let Some(segment) = self.segments.back_mut() {
                    segment.from = Some(pts);
                }
                self.frontier = Some(pts);
            }
        }
        value
    }

    /// A step: content after the frontier gets the new `T`.
    pub fn jump(&mut self, value: Timestamp) {
        self.set(value, 0.0, value);
    }

    /// A ramp from the frontier on, by `rate` per second of content until `target` is reached.
    /// Rate 0 holds `T` where it is.
    pub fn slew(&mut self, rate: f64, target: Timestamp) {
        let value = self.value();
        let target = match rate == 0.0 {
            true => value,
            false => target,
        };
        self.set(value, rate, target);
    }

    /// What holds for the content after the frontier.
    fn set(&mut self, value: Timestamp, rate: f64, target: Timestamp) {
        let current = self.value();
        let frontier = self.frontier;
        let is_first = self.segments.len() == 1;
        let Some(last) = self.segments.back_mut() else {
            return;
        };
        if current == value && last.rate == rate && last.target == target {
            return;
        }
        // Nothing was released since it started. The first one also holds for the content at
        // its `from`.
        let unused = match is_first {
            true => frontier.is_none(),
            false => last.from == frontier,
        };
        if unused {
            *last = Segment {
                from: last.from,
                value,
                rate,
                target,
            };
            return;
        }
        self.segments.push_back(Segment {
            from: self.frontier,
            value,
            rate,
            target,
        });

        if let Some(frontier) = self.frontier {
            while let Some(second) = self.segments.get(1)
                && second.from.is_some_and(|from| from + HISTORY < frontier)
            {
                self.segments.pop_front();
            }
        }
    }

    /// Whether a re-anchor still changes the `T` of this pts.
    pub fn is_open(&self, pts: Timestamp) -> bool {
        self.frontier.is_none_or(|frontier| pts > frontier) || self.is_new_content(pts)
    }

    fn is_new_content(&self, pts: Timestamp) -> bool {
        self.frontier
            .is_some_and(|frontier| pts + HISTORY < frontier)
    }
}
