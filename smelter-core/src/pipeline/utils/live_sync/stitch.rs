use std::{collections::VecDeque, time::Duration};

use crate::{Timestamp, pipeline::utils::input_sync::TrackKind};

/// ST1: the pts went back by more than this. No stall can do that.
const BACKWARD: Duration = Duration::from_secs(1);

/// ST1: the content skipped ahead by more than this beyond the time that passed.
const FORWARD: Duration = Duration::from_secs(10);

/// ST4: how much content the other track delivers before a re-base that found no pair is
/// undone.
const PAIR_WINDOW: Duration = Duration::from_secs(2);

/// How many chunks the frame gap of a track is read from.
const RECENT_CHUNKS: usize = 8;

/// What stage 0 decided for one chunk.
#[derive(Default)]
pub(super) struct Stitched {
    /// Added to the raw pts of the chunk.
    pub offset: Timestamp,
    /// The chunk does not continue what the track delivered before it.
    pub discontinuity: bool,
}

/// ST2: a re-base of one track that waits for the other track to jump the same way.
struct Rebase {
    kind: TrackKind,
    offset: Timestamp,
    /// `J`: how far the content of the track moved, relative to the offset of the input.
    jump: Timestamp,
    /// ST4: raw pts of the first chunk the other track delivered since.
    other_since: Option<Timestamp>,
}

struct TrackStitch {
    /// Raw pts and arrival of the previous chunk.
    last_pts: Timestamp,
    last_arrival: Timestamp,
    /// Largest stitched pts so far.
    newest: Timestamp,
    /// Raw pts of the chunks since the last jump.
    recent: VecDeque<Timestamp>,
    /// ST4: its re-base was undone, which its next chunk tells the sink.
    undone: bool,
}

impl TrackStitch {
    /// ST2: the offset with which a chunk continues the content of the track.
    fn continuing_offset(&self, pts: Timestamp) -> Timestamp {
        let frame_gap = self
            .recent
            .iter()
            .zip(self.recent.iter().skip(1))
            .map(|(previous, next)| *next - *previous)
            .filter(|step| step.is_positive())
            .min()
            .unwrap_or(Timestamp::ZERO);
        self.newest + frame_gap - pts
    }
}

/// Stage 0: stitches pts discontinuities of an input, so that the delivery delay is continuous
/// over them. Tracks jump one after another, and the second one takes over the offset of the
/// first, which keeps the relation between the tracks.
#[derive(Default)]
pub(super) struct Stitch {
    /// Offset of the input: what its tracks use, apart from the one with a waiting re-base.
    offset: Timestamp,
    waiting: Option<Rebase>,
    audio: Option<TrackStitch>,
    video: Option<TrackStitch>,
}

impl Stitch {
    /// `has_other`: the input has a second track.
    pub fn observe(
        &mut self,
        kind: TrackKind,
        pts: Timestamp,
        arrival: Timestamp,
        has_other: bool,
    ) -> Stitched {
        let track = match kind {
            TrackKind::Audio => &mut self.audio,
            TrackKind::Video => &mut self.video,
        };
        let Some(track) = track else {
            // The first chunk of a track settles a re-base that waits for it.
            if let Some(rebase) = self.waiting.take() {
                self.offset = rebase.offset;
            }
            *track = Some(TrackStitch {
                last_pts: pts,
                last_arrival: arrival,
                newest: pts + self.offset,
                recent: VecDeque::from([pts]),
                undone: false,
            });
            return Stitched {
                offset: self.offset,
                discontinuity: false,
            };
        };

        // ST1
        let step = pts - track.last_pts;
        let jump = step - (arrival - track.last_arrival);
        let is_jump = step < -Timestamp::from(BACKWARD) || jump > Timestamp::from(FORWARD);
        let mut discontinuity = std::mem::take(&mut track.undone);

        match &mut self.waiting {
            // this track waits and jumps again
            Some(rebase) if rebase.kind == kind => {
                if is_jump {
                    discontinuity = true;
                    let total = rebase.jump + jump;
                    if total.abs() <= pair_tolerance(rebase.jump) {
                        // away and back: a broken timestamp
                        self.waiting = None;
                    } else {
                        rebase.offset = track.continuing_offset(pts);
                        rebase.jump = total;
                        rebase.other_since = None;
                    }
                }
            }
            // the other track waits
            Some(rebase) => {
                let same_way = !rebase.jump.is_negative() || step.is_negative();
                let pairs = (jump - rebase.jump).abs() <= pair_tolerance(rebase.jump) && same_way;
                let since = *rebase.other_since.get_or_insert(pts);
                if pairs {
                    // ST3: it takes over the offset, so the relation of the tracks is kept
                    discontinuity = true;
                    self.offset = rebase.offset;
                    self.waiting = None;
                } else if is_jump {
                    // An unrelated jump: what waited becomes the offset of the input, and this
                    // one waits in its place.
                    discontinuity = true;
                    self.offset = rebase.offset;
                    self.waiting = Some(Rebase {
                        kind,
                        offset: track.continuing_offset(pts),
                        jump: jump - rebase.jump,
                        other_since: None,
                    });
                } else if pts - since >= Timestamp::from(PAIR_WINDOW) {
                    // ST4: it was a gap in one track, not a new timeline
                    let undone = match rebase.kind {
                        TrackKind::Audio => &mut self.audio,
                        TrackKind::Video => &mut self.video,
                    };
                    if let Some(undone) = undone {
                        undone.undone = true;
                    }
                    self.waiting = None;
                }
            }
            None => {
                if is_jump {
                    // ST2
                    discontinuity = true;
                    let offset = track.continuing_offset(pts);
                    match has_other {
                        true => {
                            self.waiting = Some(Rebase {
                                kind,
                                offset,
                                jump,
                                other_since: None,
                            })
                        }
                        false => self.offset = offset,
                    }
                }
            }
        }

        let offset = match &self.waiting {
            Some(rebase) if rebase.kind == kind => rebase.offset,
            _ => self.offset,
        };
        let track = match kind {
            TrackKind::Audio => &mut self.audio,
            TrackKind::Video => &mut self.video,
        };
        if let Some(track) = track {
            if discontinuity {
                track.recent.clear();
            }
            if track.recent.len() == RECENT_CHUNKS {
                track.recent.pop_front();
            }
            track.recent.push_back(pts);
            track.last_pts = pts;
            track.last_arrival = arrival;
            track.newest = Timestamp::max(track.newest, pts + offset);
        }
        Stitched {
            offset,
            discontinuity,
        }
    }

    pub fn remove_track(&mut self, kind: TrackKind) {
        // one track is left, so nothing waits
        if let Some(rebase) = self.waiting.take()
            && rebase.kind != kind
        {
            self.offset = rebase.offset;
        }
        match kind {
            TrackKind::Audio => self.audio = None,
            TrackKind::Video => self.video = None,
        }
    }
}

/// ST3: how close two jumps have to be to belong together.
fn pair_tolerance(jump: Timestamp) -> Timestamp {
    Timestamp::min(Timestamp::from_secs(1), jump.abs() / 2i64)
}
