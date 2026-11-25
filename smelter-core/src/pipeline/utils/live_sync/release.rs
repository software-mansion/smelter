use std::time::Duration;

use tracing::{debug, trace};

use super::{CAPACITY, buffer::LiveSyncBuffer, decide::Playback, state::Track, timeline::Timeline};
use crate::{
    Timestamp,
    pipeline::utils::input_sync::{InputSyncItem, TrackEvent, TrackKind},
};

/// RL2: how long before its presentation time a chunk is released. Until then a re-anchor
/// still reaches it; the decoder gets this much time.
const AHEAD: Duration = Duration::from_millis(500);

/// Where a buffered chunk is relative to its presentation time.
enum Due {
    /// Not within `AHEAD` yet.
    Later,
    OnTime,
    /// RL4: late, but a restart at where delivery is now would still play it.
    Kept,
    Late,
}

impl<B: LiveSyncBuffer> Track<B> {
    fn due(&self, pts: Timestamp, playback: &Playback, now: Timestamp) -> Due {
        let presented = pts + playback.timeline.at(pts);
        if presented > now + AHEAD {
            return Due::Later;
        }
        if presented >= now && self.floor.is_none_or(|floor| presented >= floor) {
            return Due::OnTime;
        }
        match playback.restart {
            Some(restart) if playback.timeline.is_open(pts) && pts + restart >= now => Due::Kept,
            _ => Due::Late,
        }
    }

    /// Stage 3: releases the chunks that are due, in the order of the buffer, and handles the
    /// ones that are late.
    pub(super) fn release(&mut self, playback: &mut Playback, now: Timestamp) {
        loop {
            let next = self.buffer.iter().next();
            let Some((pts, is_keyframe)) = next.map(|next| (next.pts(), next.is_keyframe())) else {
                break;
            };
            if self.needs_keyframe && !is_keyframe {
                trace!(kind=?self.kind, ?pts, "Live sync: dropping chunk, waiting for a keyframe");
                self.drop_next();
                continue;
            }
            match (self.due(pts, playback, now), self.kind) {
                (Due::Later | Due::Kept, _) => break,
                (Due::OnTime, _) => self.release_next(&mut playback.timeline, now, true),
                (Due::Late, TrackKind::Audio) => {
                    trace!(?pts, "Live sync: dropping late audio chunk");
                    self.drop_next();
                }
                (Due::Late, TrackKind::Video) => self.release_late_video(playback, now),
            }
        }
    }

    /// RL4: late video is cut back to its newest keyframe. Late GOPs that are followed by a
    /// newer keyframe are dropped. The rest is released decode-only, so the decoder is ready
    /// for the first chunk that is on time; when nothing follows, the newest late frame is
    /// shown.
    fn release_late_video(&mut self, playback: &mut Playback, now: Timestamp) {
        let mut late = 0;
        let mut newest_keyframe = 0;
        let mut ends_buffer = true;
        for (position, chunk) in self.buffer.iter().enumerate() {
            if !matches!(self.due(chunk.pts(), playback, now), Due::Late) {
                ends_buffer = false;
                break;
            }
            if chunk.is_keyframe() {
                newest_keyframe = position;
            }
            late = position + 1;
        }
        if newest_keyframe > 0 {
            debug!(chunks = newest_keyframe, "Live sync: dropping late video");
        }
        for position in 0..late {
            if position < newest_keyframe {
                self.drop_next();
                continue;
            }
            let shown = ends_buffer && position + 1 == late;
            if shown {
                self.stats.report_late_chunk_shown();
            }
            self.release_next(&mut playback.timeline, now, shown);
        }
    }

    /// RL6: releases everything that is left, late or not.
    pub(super) fn flush(&mut self, timeline: &mut Timeline, now: Timestamp) {
        loop {
            let next = self.buffer.iter().next();
            let Some(is_keyframe) = next.map(|next| next.is_keyframe()) else {
                break;
            };
            match self.needs_keyframe && !is_keyframe {
                true => self.drop_next(),
                false => self.release_next(timeline, now, true),
            }
        }
    }

    /// RL5: beyond the capacity the oldest content is dropped. `newest`: pts of the chunk that
    /// was just written.
    pub(super) fn enforce_capacity(&mut self, newest: Timestamp) {
        loop {
            let oldest = self.buffer.iter().next().map(|oldest| oldest.pts());
            if oldest.is_none_or(|oldest| oldest + CAPACITY >= newest) {
                break;
            }
            debug!(kind=?self.kind, "Live sync: buffer is full, dropping the oldest chunk");
            self.drop_next();
            self.needs_keyframe = true;
        }
    }

    fn drop_next(&mut self) {
        if self.take().is_some() {
            self.stats.report_chunk_dropped();
        }
    }

    /// Releases the next chunk of the buffer on `timeline`. `shown: false` releases it only
    /// to be decoded.
    fn release_next(&mut self, timeline: &mut Timeline, now: Timestamp, shown: bool) {
        let Some(mut chunk) = self.take() else {
            return;
        };
        let input_pts = chunk.pts();
        let offset = timeline.release(input_pts);
        chunk.apply_anchor(Timestamp::offset(Timestamp::ZERO, offset));
        let presented = chunk.pts();
        match shown {
            true => {
                self.released_until = Some(match self.released_until {
                    Some(until) => Timestamp::max(until, presented),
                    None => presented,
                })
            }
            false => chunk.mark_decode_only(),
        }
        self.needs_keyframe = false;

        trace!(
            kind=?self.kind,
            ?input_pts,
            ?presented,
            lead=?(presented - now),
            shown,
            "Live sync: releasing chunk"
        );
        self.stats.report_chunk_released(presented - now);
        self.sink.on_event(TrackEvent::Chunk(chunk));
    }

    /// Takes the next chunk out of the buffer. A discontinuity that was stitched in front of
    /// it (ST2) goes to the sink first.
    fn take(&mut self) -> Option<B::Chunk> {
        let chunk = self.buffer.pop()?;
        while let Some(position) = self.discontinuities.front()
            && *position <= self.taken
        {
            self.discontinuities.pop_front();
            self.sink.on_event(TrackEvent::Discontinuity);
            self.stats.report_discontinuity();
        }
        self.taken += 1;
        Some(chunk)
    }
}
