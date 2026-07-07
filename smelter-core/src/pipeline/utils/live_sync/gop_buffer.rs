use std::{collections::VecDeque, time::Duration};

/// Transient packet hold used while probing for the live edge.
///
/// Keeps only content that is still a valid join point: every keyframe
/// makes older items obsolete (joining always happens at the newest
/// keyframe), so the buffer clears itself on each keyframe push. For
/// streams without keyframes (audio) it degrades to a duration-bounded
/// hold; packets older than the eventually chosen baseline are dropped
/// at flush by the mapper (`BeforeBaseline`).
pub(crate) struct GopBuffer<T> {
    items: VecDeque<(Duration, T)>,
    keyframe_pts: Option<Duration>,
    max_duration: Duration,
}

impl<T> GopBuffer<T> {
    pub fn new(max_duration: Duration) -> Self {
        Self {
            items: VecDeque::new(),
            keyframe_pts: None,
            max_duration,
        }
    }

    pub fn push(&mut self, pts: Duration, keyframe: bool, item: T) {
        if keyframe {
            self.items.clear();
            self.keyframe_pts = Some(pts);
        }
        self.items.push_back((pts, item));

        // bound memory when no keyframe shows up for a long time
        while let (Some((front_pts, _)), Some((back_pts, _))) =
            (self.items.front(), self.items.back())
        {
            if back_pts.saturating_sub(*front_pts) <= self.max_duration {
                break;
            }
            if self.keyframe_pts == Some(*front_pts) {
                self.keyframe_pts = None;
            }
            self.items.pop_front();
        }
    }

    /// PTS of the newest keyframe; the join point of the held content.
    pub fn keyframe_pts(&self) -> Option<Duration> {
        self.keyframe_pts
    }

    pub fn newest_pts(&self) -> Option<Duration> {
        self.items.back().map(|(pts, _)| *pts)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn take(&mut self) -> VecDeque<(Duration, T)> {
        self.keyframe_pts = None;
        std::mem::take(&mut self.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn keeps_only_newest_gop() {
        let mut buffer = GopBuffer::new(ms(10_000));
        buffer.push(ms(0), true, "kf0");
        buffer.push(ms(33), false, "p1");
        buffer.push(ms(66), false, "p2");
        buffer.push(ms(2000), true, "kf1");
        buffer.push(ms(2033), false, "p3");

        assert_eq!(buffer.keyframe_pts(), Some(ms(2000)));
        let items: Vec<_> = buffer.take().into_iter().map(|(_, item)| item).collect();
        assert_eq!(items, vec!["kf1", "p3"]);
        assert!(buffer.is_empty());
        assert_eq!(buffer.keyframe_pts(), None);
    }

    #[test]
    fn bounds_duration_without_keyframes() {
        let mut buffer = GopBuffer::new(ms(1000));
        for i in 0..100 {
            buffer.push(ms(i * 100), false, i);
        }
        let items = buffer.take();
        assert_eq!(items.front().unwrap().0, ms(8900));
        assert_eq!(items.back().unwrap().0, ms(9900));
    }

    #[test]
    fn drops_keyframe_pts_when_keyframe_trimmed_out() {
        let mut buffer = GopBuffer::new(ms(1000));
        buffer.push(ms(0), true, 0);
        for i in 1..50 {
            buffer.push(ms(i * 100), false, i);
        }
        assert_eq!(buffer.keyframe_pts(), None);
        assert_eq!(buffer.newest_pts(), Some(ms(4900)));
    }
}
