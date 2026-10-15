use std::{collections::BTreeMap, time::Duration};

use crate::{Timestamp, pipeline::utils::input_sync::TrackKind, stats::utils::SlidingWindowValue};

/// Chunks arriving later than this behind the largest pts are a timestamp discontinuity, not
/// reordering.
const MAX_DELAY: Duration = Duration::from_secs(10);

/// Events sent by decoders for every chunk of encoded media.
#[derive(Debug, Clone, Copy)]
pub(crate) enum InputMediaStatsEvent {
    ChunkReceived { size: usize, pts: Timestamp },
}

/// Media stats of the input tracks, measured at the decoder input.
#[derive(Debug)]
pub struct InputMediaStatsState {
    pub video: TrackMediaStatsState,
    pub audio: TrackMediaStatsState,
}

#[derive(Debug)]
pub struct TrackMediaStatsState {
    received_any: bool,
    max_pts: Option<Timestamp>,
    /// How far behind the largest pts received so far the chunks arrive.
    delay: SlidingWindowValue<Duration>,
    /// Size in bytes of the received chunks by pts.
    chunks: BTreeMap<Timestamp, u64>,
}

impl InputMediaStatsState {
    pub fn new() -> Self {
        Self {
            video: TrackMediaStatsState::new(),
            audio: TrackMediaStatsState::new(),
        }
    }

    pub fn handle_event(&mut self, track: TrackKind, event: InputMediaStatsEvent) {
        match track {
            TrackKind::Video => self.video.handle_event(event),
            TrackKind::Audio => self.audio.handle_event(event),
        }
    }
}

impl TrackMediaStatsState {
    fn new() -> Self {
        Self {
            received_any: false,
            max_pts: None,
            delay: SlidingWindowValue::new(Duration::from_mins(1)),
            chunks: BTreeMap::new(),
        }
    }

    pub fn received_any(&self) -> bool {
        self.received_any
    }

    pub fn bitrate_1_second(&mut self) -> u64 {
        self.bitrate(Duration::from_secs(1))
    }

    pub fn bitrate_1_minute(&mut self) -> u64 {
        self.bitrate(Duration::from_mins(1))
    }

    /// Bits of the chunks in the `window` of pts ending at the max delay behind the largest pts,
    /// so chunks that are still late are not missing from it.
    fn bitrate(&mut self, window: Duration) -> u64 {
        let Some(max_pts) = self.max_pts else {
            return 0;
        };
        let end = max_pts - self.delay.max();
        let bytes: u64 = self
            .chunks
            .range(end - window..end)
            .map(|(_, size)| size)
            .sum();
        (8.0 * bytes as f64 / window.as_secs_f64()) as u64
    }

    fn handle_event(&mut self, event: InputMediaStatsEvent) {
        match event {
            InputMediaStatsEvent::ChunkReceived { size, pts } => {
                self.received_any = true;
                let max_pts = self.max_pts.unwrap_or(pts);
                let delay = (max_pts - pts).to_duration_saturating();
                let max_pts = match delay > MAX_DELAY {
                    true => {
                        self.chunks.clear();
                        pts
                    }
                    false => {
                        if !delay.is_zero() {
                            self.delay.push(delay);
                        }
                        Timestamp::max(max_pts, pts)
                    }
                };
                self.max_pts = Some(max_pts);
                *self.chunks.entry(pts).or_default() += size as u64;

                // 1 minute is the largest bitrate window requested.
                let cutoff = max_pts - MAX_DELAY - Duration::from_mins(1);
                while self
                    .chunks
                    .first_key_value()
                    .is_some_and(|(pts, _)| *pts < cutoff)
                {
                    self.chunks.pop_first();
                }
            }
        }
    }
}
