use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    task::Poll,
    time::Duration,
};

use moq_mux::{catalog::hang::Container, container::Container as _};
use moq_native::moq_net::{GroupConsumer, TrackConsumer, kio};
use tracing::{debug, warn};

use super::super::jitter_buffer::{GroupInfo, MoqChunk};
use crate::pipeline::utils::live_sync::LiveSyncDeadline;

use crate::prelude::*;

/// When a group that is still being delivered is given up on. A group ends where the next group
/// starts, so it is judged by the start of the nearest later group.
#[derive(Debug, Clone, Copy)]
pub(super) struct LateGroupOptions {
    /// How far past the deadline of the live sync a group is still read. Its frames cannot be
    /// played anymore, but their late arrival lets the live sync learn that the buffer is too
    /// small.
    pub grace: Duration,
    /// A group ending further behind the newest group is dropped whether or not the live sync
    /// has a deadline. Bounds how far the buffer can grow on late deliveries.
    pub max_behind: Duration,
}

/// Reads the frames of a track as they arrive. Every open group is polled, so a group that
/// stalls does not hold back the ones after it; ordering is left to the jitter buffer.
pub(super) struct MoqTrackReader {
    track: TrackConsumer,
    container: Container,
    kind: MediaKind,
    deadline: LiveSyncDeadline,
    options: LateGroupOptions,
    /// Open groups by sequence.
    groups: BTreeMap<u64, TrackGroup>,
    /// Pts of the first frame of every group after the oldest open one; where the open groups
    /// end.
    group_starts: BTreeMap<u64, Timestamp>,
    ready: VecDeque<MoqChunk>,
    track_finished: bool,
}

struct TrackGroup {
    consumer: GroupConsumer,
    /// Index of the next frame.
    index: u32,
    info: Arc<GroupInfo>,
}

impl MoqTrackReader {
    pub fn new(
        track: TrackConsumer,
        container: Container,
        kind: MediaKind,
        deadline: LiveSyncDeadline,
        options: LateGroupOptions,
    ) -> Self {
        Self {
            track,
            container,
            kind,
            deadline,
            options,
            groups: BTreeMap::new(),
            group_starts: BTreeMap::new(),
            ready: VecDeque::new(),
            track_finished: false,
        }
    }

    /// Next frame in arrival order; `None` once the track has ended.
    pub async fn read(&mut self) -> Result<Option<MoqChunk>, moq_mux::Error> {
        kio::wait(|waiter| self.poll_read(waiter)).await
    }

    fn poll_read(
        &mut self,
        waiter: &kio::Waiter,
    ) -> Poll<Result<Option<MoqChunk>, moq_mux::Error>> {
        if let Some(chunk) = self.ready.pop_front() {
            return Poll::Ready(Ok(Some(chunk)));
        }
        self.poll_new_groups(waiter)?;
        self.poll_groups(waiter);
        self.drop_late_groups();
        match self.ready.pop_front() {
            Some(chunk) => Poll::Ready(Ok(Some(chunk))),
            None if self.track_finished && self.groups.is_empty() => Poll::Ready(Ok(None)),
            None => Poll::Pending,
        }
    }

    fn poll_new_groups(&mut self, waiter: &kio::Waiter) -> Result<(), moq_mux::Error> {
        while !self.track_finished {
            match self.track.poll_recv_group(waiter) {
                Poll::Ready(Ok(Some(group))) => {
                    self.groups.insert(
                        group.sequence,
                        TrackGroup {
                            consumer: group,
                            index: 0,
                            info: Arc::default(),
                        },
                    );
                }
                Poll::Ready(Ok(None)) => self.track_finished = true,
                Poll::Ready(Err(err)) => return Err(err.into()),
                Poll::Pending => break,
            }
        }
        Ok(())
    }

    /// Drains every frame that is available; groups that ended are removed.
    fn poll_groups(&mut self, waiter: &kio::Waiter) {
        let Self {
            container,
            kind,
            groups,
            group_starts,
            ready,
            ..
        } = self;
        groups.retain(|sequence, group| {
            loop {
                match container.poll_read(&mut group.consumer, waiter) {
                    Poll::Ready(Ok(Some(frames))) => {
                        for frame in frames {
                            let pts = Timestamp::from_micros(frame.timestamp.as_micros() as i64);
                            if group.index == 0 {
                                group_starts.insert(*sequence, pts);
                            }
                            ready.push_back(MoqChunk {
                                chunk: EncodedInputChunk {
                                    data: frame.payload,
                                    pts,
                                    dts: None,
                                    kind: *kind,
                                    decode_only: false,
                                },
                                group: *sequence,
                                index: group.index,
                                group_info: group.info.clone(),
                            });
                            group.index += 1;
                        }
                    }
                    Poll::Ready(Ok(None)) => {
                        group.info.finalize_group_len(group.index);
                        return false;
                    }
                    Poll::Ready(Err(err)) => {
                        warn!(%err, group = sequence, "Failed to read moq group");
                        group.info.finalize_group_len(group.index);
                        return false;
                    }
                    Poll::Pending => return true,
                }
            }
        });
    }

    /// Gives up on open groups whose end is behind the deadline (plus grace) or too far behind
    /// the newest group. A group without a known end is kept.
    fn drop_late_groups(&mut self) {
        let deadline = self.deadline.get();
        let newest_start_pts = self.group_starts.last_key_value().map(|(_, pts)| *pts);

        self.groups.retain(|sequence, group| {
            let next_group_start_pts = self.group_starts.range(sequence + 1..).next();
            let Some((_, next_group_start_pts)) = next_group_start_pts else {
                return true;
            };

            // Group is already behind the queue. Extra grace is added so estimator can observe
            // late packet and nudge the buffer and because anchors can move in the meantime
            let past_deadline = deadline
                .is_some_and(|deadline| *next_group_start_pts + self.options.grace < deadline);

            // In case deadline anchor is invalid/outdated fallback to max_behind relative
            // to the newest group we have seen
            let too_far_behind = newest_start_pts
                .is_some_and(|newest| *next_group_start_pts + self.options.max_behind < newest);
            if !past_deadline && !too_far_behind {
                return true;
            }
            debug!(
                group = sequence,
                ?next_group_start_pts,
                ?deadline,
                "Dropping late moq group"
            );
            group.info.finalize_group_len(group.index);
            false
        });

        // We still need time information for groups that already completed, removing
        // only groups older than last open one.
        match self.groups.keys().next().copied() {
            Some(oldest_open) => self
                .group_starts
                .retain(|sequence, _| *sequence > oldest_open),
            None => self.group_starts.clear(),
        }
    }
}
