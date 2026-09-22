use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

use tracing::trace;

use crate::{
    pipeline::utils::{input_sync::InputSyncItem, live_sync::LiveSyncBuffer},
    stats::LiveSyncBufferStats,
};

use crate::prelude::*;

/// Shared between the track reader and the chunks of one group.
#[derive(Default)]
pub(super) struct GroupInfo {
    /// Frame count, set once the end of the group is received.
    len: OnceLock<u32>,
}

impl GroupInfo {
    pub fn finalize_group_len(&self, len: u32) {
        _ = self.len.set(len);
    }
}

impl std::fmt::Debug for GroupInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupInfo").field("len", &self.len).finish()
    }
}

/// Frame of a moq track with its position in the track.
#[derive(Debug)]
pub(super) struct MoqChunk {
    pub chunk: EncodedInputChunk,
    pub group: u64,
    /// Index of the frame within the group.
    pub index: u32,
    pub group_info: Arc<GroupInfo>,
}

impl InputSyncItem for MoqChunk {
    fn pts(&self) -> Timestamp {
        self.chunk.pts()
    }

    fn size(&self) -> usize {
        self.chunk.size()
    }

    fn apply_anchor(&mut self, anchor: TimestampOffset) {
        self.chunk.apply_anchor(anchor);
    }

    fn mark_decode_only(&mut self) {
        self.chunk.mark_decode_only();
    }
}

/// Orders frames by group and index. Frames of a group arrive in order, so the only gaps are a
/// group that is still being delivered and a group that has not arrived at all. `try_read` holds
/// the next group back behind both; `read` gives up on the missing frames.
#[derive(Default)]
pub(super) struct MoqJitterBuffer {
    chunks: BTreeMap<(u64, u32), MoqChunk>,
    last_released: Option<Position>,
}

struct Position {
    group: u64,
    index: u32,
    group_info: Arc<GroupInfo>,
}

impl Position {
    /// Whether `chunk` comes right after this frame: the next frame of the same group, or the
    /// first frame of the next group when this group is known to end here.
    fn is_followed_by(&self, chunk: &MoqChunk) -> bool {
        let same_group = chunk.group == self.group && chunk.index == self.index + 1;
        let next_group = chunk.group == self.group + 1
            && chunk.index == 0
            && self.group_info.len.get() == Some(&(self.index + 1));
        same_group || next_group
    }
}

impl MoqJitterBuffer {
    fn next_is_continuation(&self) -> bool {
        match (&self.last_released, self.chunks.values().next()) {
            (Some(last), Some(chunk)) => last.is_followed_by(chunk),
            (None, Some(_)) => true,
            (_, None) => false,
        }
    }

    fn pop_first(&mut self) -> Option<MoqChunk> {
        let (_, chunk) = self.chunks.pop_first()?;
        self.last_released = Some(Position {
            group: chunk.group,
            index: chunk.index,
            group_info: chunk.group_info.clone(),
        });
        Some(chunk)
    }
}

impl LiveSyncBuffer for MoqJitterBuffer {
    type Chunk = MoqChunk;

    fn write(&mut self, chunk: MoqChunk) {
        let position = (chunk.group, chunk.index);
        let is_late = self
            .last_released
            .as_ref()
            .is_some_and(|last| position <= (last.group, last.index));
        if is_late {
            trace!(?position, "Dropping late moq chunk");
            return;
        }
        self.chunks.insert(position, chunk);
    }

    fn read(&mut self) -> Option<MoqChunk> {
        self.pop_first()
    }

    fn try_read(&mut self) -> Option<MoqChunk> {
        match self.next_is_continuation() {
            true => self.pop_first(),
            false => None,
        }
    }

    fn peek_next_pts(&self) -> Option<Timestamp> {
        self.chunks.values().next().map(|chunk| chunk.pts())
    }

    fn stats(&self) -> LiveSyncBufferStats {
        let min = self.chunks.values().map(|chunk| chunk.pts()).min();
        let max = self.chunks.values().map(|chunk| chunk.pts()).max();
        LiveSyncBufferStats::Jitter {
            duration: match (min, max) {
                (Some(min), Some(max)) => max - min,
                _ => Timestamp::ZERO,
            },
            waiting_for_gap: !self.chunks.is_empty() && !self.next_is_continuation(),
        }
    }
}
