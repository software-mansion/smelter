use std::time::Duration;

use crate::pipeline::utils::input_sync::InputSyncItem;

/// Share of the mapped media a correction may consume: between two
/// consecutive chunks the mapping moves by at most 4% of their pts distance,
/// so the rate of output timestamps changes by at most 4% and, staying below
/// 100%, never reverses.
const SLEW_RATE_PERCENT: u32 = 4;

/// Largest shift a single chunk may apply. Chunks carrying a lot of media (an
/// HLS segment, a pts gap below the discontinuity threshold) would otherwise
/// spend their whole allowance on one chunk boundary.
const MAX_SLEW_STEP: Duration = Duration::from_millis(40);

/// Correspondence between the input and output timelines chosen when a track
/// starts producing chunks: content at `input_pts` is presented at
/// `output_pts`, and every other timestamp keeps its distance to the anchor.
#[derive(Debug, Clone, Copy)]
pub(super) struct TimestampAnchor {
    /// Raw pts of the anchor: the pts presented right after the start, or the
    /// oldest buffered pts on flush.
    pub input_pts: Duration,
    /// Pts relative to the sync point at which content at `input_pts` is
    /// presented.
    pub output_pts: Duration,
}

impl TimestampAnchor {
    /// Maps a raw timestamp (pts or dts) onto the sync point timeline.
    /// Timestamps below `input_pts` (initial backlog) map before the start
    /// point, saturating at zero; such content plays late or is dropped by
    /// the consumer.
    pub(super) fn to_output_pts(&self, pts: Duration) -> Duration {
        (self.output_pts + pts).saturating_sub(self.input_pts)
    }
}

/// Timestamp mapping of a started track.
///
/// A correction does not replace the mapping outright: [`correct_to`] only
/// picks the anchor the mapping approaches, and every mapped chunk moves it
/// one step closer. The step is a share of the media the chunk advanced by,
/// so the correction is spread over the content it applies to instead of
/// landing on a single chunk boundary.
///
/// [`correct_to`]: Self::correct_to
pub(super) struct SlewingAnchor {
    anchor: TimestampAnchor,
    /// Anchor of a correction in progress; reaching it ends the correction.
    destination: Option<TimestampAnchor>,
    /// Largest pts mapped so far. The media a chunk advances by is measured
    /// from it, so chunks delivered out of presentation order do not spend
    /// the same allowance twice.
    last_pts: Option<Duration>,
}

impl SlewingAnchor {
    pub(super) fn new(anchor: TimestampAnchor) -> Self {
        Self {
            anchor,
            destination: None,
            last_pts: None,
        }
    }

    /// Mapping as of the last mapped chunk.
    pub(super) fn current(&self) -> TimestampAnchor {
        self.anchor
    }

    pub(super) fn is_correcting(&self) -> bool {
        self.destination.is_some()
    }

    /// Starts approaching `destination`, replacing a correction in progress.
    /// It is reached once enough media has been mapped.
    pub(super) fn correct_to(&mut self, destination: TimestampAnchor) {
        self.destination = Some(destination);
    }

    /// Maps the timestamps of `chunk` onto the sync point timeline, moving a
    /// correction in progress one step closer.
    pub(super) fn map_chunk(&mut self, chunk: &mut impl InputSyncItem) {
        let pts = chunk.pts();
        self.slew(pts);
        self.last_pts = Some(self.last_pts.map_or(pts, |last| Duration::max(last, pts)));

        let anchor = self.anchor;
        chunk.map_timestamps(|pts| anchor.to_output_pts(pts));
    }

    /// Moves the mapping towards the destination by a share of the media
    /// between `pts` and the previously mapped chunk.
    fn slew(&mut self, pts: Duration) {
        let Some(destination) = self.destination else {
            return;
        };
        let media = self
            .last_pts
            .map_or(Duration::ZERO, |last| pts.saturating_sub(last));
        let step = Duration::min(media * SLEW_RATE_PERCENT / 100, MAX_SLEW_STEP);

        // expressed at the same input pts, the mappings differ only by their
        // output pts
        let current = self.anchor.to_output_pts(destination.input_pts);
        let output_pts = match current < destination.output_pts {
            true => Duration::min(current + step, destination.output_pts),
            false => Duration::max(current.saturating_sub(step), destination.output_pts),
        };
        self.anchor = TimestampAnchor {
            input_pts: destination.input_pts,
            output_pts,
        };
        if output_pts == destination.output_pts {
            self.destination = None;
        }
    }
}
