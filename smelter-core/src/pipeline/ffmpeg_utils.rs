use std::{collections::HashMap, time::Duration};

use ffmpeg_next::{Dictionary, StreamMut, ffi::AVCodecParameters};
use tracing::warn;

use crate::{prelude::*, queue::QueueContext};

const OFFSET_RESOLUTION_TIMEOUT: Duration = Duration::from_millis(500);

/// Timestamp offset subtracted from every chunk on output.
///
/// Offset is resolved in this order:
/// - `start_at` - output start in pts counted from queue start. Packets are in units relative
///   to queue sync_point, so offset is `start_at + queue_ctx.start_pts`
/// - If only one track is present, take offset from first packet
/// - If both tracks are present, buffer until first packet of each kind is present and select
///   the lowest
///   - Wait at most `OFFSET_RESOLUTION_TIMEOUT` for that, if not fallback to first pts
pub(crate) struct TimestampOffset {
    queue_ctx: QueueContext,
    /// Relative to the queue start, not to the sync point the chunk PTS use.
    start_at: Option<Duration>,
    state: State,
}

enum State {
    Resolved(Duration),
    Pending {
        waiting_for_video: bool,
        waiting_for_audio: bool,
        lowest_pts: Option<Duration>,
        buffered: Vec<EncodedOutputChunk>,
    },
}

impl TimestampOffset {
    pub fn new(
        queue_ctx: QueueContext,
        start_at: Option<Duration>,
        has_video: bool,
        has_audio: bool,
    ) -> Self {
        Self {
            queue_ctx,
            start_at,
            // Not resolved here even when `start_at` is set, we need to wait for queue start
            state: State::Pending {
                waiting_for_video: has_video,
                waiting_for_audio: has_audio,
                lowest_pts: None,
                buffered: Vec::new(),
            },
        }
    }

    /// Chunks ready to be written, each with the offset to apply. Empty while the offset
    /// is still pending, in which case the chunk is buffered until it resolves.
    pub fn resolve(&mut self, chunk: EncodedOutputChunk) -> Vec<(Duration, EncodedOutputChunk)> {
        let (waiting_for_video, waiting_for_audio, lowest_pts, buffered) = match &mut self.state {
            State::Resolved(offset) => return vec![(*offset, chunk)],
            State::Pending {
                waiting_for_video,
                waiting_for_audio,
                lowest_pts,
                buffered,
            } => (waiting_for_video, waiting_for_audio, lowest_pts, buffered),
        };

        match chunk.kind {
            MediaKind::Video(_) => *waiting_for_video = false,
            MediaKind::Audio(_) => *waiting_for_audio = false,
        }
        let lowest = match *lowest_pts {
            Some(lowest) => Duration::min(lowest, chunk.pts),
            None => chunk.pts,
        };
        *lowest_pts = Some(lowest);

        let timed_out = chunk.pts.saturating_sub(lowest) > OFFSET_RESOLUTION_TIMEOUT;
        if timed_out {
            warn!(
                ?lowest,
                waiting_for_video = *waiting_for_video,
                waiting_for_audio = *waiting_for_audio,
                "Timed out waiting for the first chunk of the other track, anchoring output timestamps on what arrived so far."
            );
        }
        buffered.push(chunk);

        let still_waiting = *waiting_for_video || *waiting_for_audio;
        if still_waiting && !timed_out {
            return Vec::new();
        }
        self.force_resolve(lowest)
    }

    /// A track ended; stop waiting for a first chunk it is never going to produce. Returns
    /// the buffered chunks if this was the last track the offset was waiting on.
    pub fn on_track_eos(&mut self, kind: MediaKind) -> Vec<(Duration, EncodedOutputChunk)> {
        let (waiting_for_video, waiting_for_audio, lowest_pts) = match &mut self.state {
            // Already anchored, so nothing was buffered.
            State::Resolved(_) => return Vec::new(),
            State::Pending {
                waiting_for_video,
                waiting_for_audio,
                lowest_pts,
                ..
            } => (waiting_for_video, waiting_for_audio, lowest_pts),
        };

        match kind {
            MediaKind::Video(_) => *waiting_for_video = false,
            MediaKind::Audio(_) => *waiting_for_audio = false,
        }
        // Nothing to anchor on yet — stay pending until some chunk shows up.
        let Some(lowest) = *lowest_pts else {
            return Vec::new();
        };
        if *waiting_for_video || *waiting_for_audio {
            return Vec::new();
        }
        self.force_resolve(lowest)
    }

    /// The packet stream ended before the offset resolved on its own. Anchor on whatever
    /// arrived so far, so buffered chunks are not lost.
    pub fn flush(&mut self) -> Vec<(Duration, EncodedOutputChunk)> {
        let lowest_pts = match &self.state {
            State::Resolved(_) => return Vec::new(),
            State::Pending { lowest_pts, .. } => *lowest_pts,
        };
        // Nothing ever arrived, so there is nothing buffered either.
        let Some(lowest) = lowest_pts else {
            return Vec::new();
        };
        self.force_resolve(lowest)
    }

    fn force_resolve(&mut self, lowest_pts: Duration) -> Vec<(Duration, EncodedOutputChunk)> {
        let offset = self
            .start_at
            .and_then(|start_at| self.queue_ctx.pts_from_start(start_at))
            .unwrap_or(lowest_pts);

        let buffered = match std::mem::replace(&mut self.state, State::Resolved(offset)) {
            State::Pending { buffered, .. } => buffered,
            State::Resolved(_) => Vec::new(),
        };
        buffered.into_iter().map(|chunk| (offset, chunk)).collect()
    }
}

#[derive(Debug, Default)]
pub(super) struct FfmpegOptions(HashMap<String, String>);

impl FfmpegOptions {
    pub fn append<T: AsRef<str>>(&mut self, options: &[(T, T)]) {
        for (key, value) in options {
            self.0
                .insert(key.as_ref().to_string(), value.as_ref().to_string());
        }
    }

    pub fn into_dictionary(self) -> Dictionary<'static> {
        Dictionary::from_iter(self.0)
    }
}

impl<T: AsRef<str>, const N: usize> From<&[(T, T); N]> for FfmpegOptions {
    fn from(value: &[(T, T); N]) -> Self {
        let mut options = FfmpegOptions::default();
        options.append(value);
        options
    }
}

pub(super) fn write_extradata(codecpar: &mut AVCodecParameters, extradata: bytes::Bytes) {
    unsafe {
        // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
        codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
            extradata.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
        ) as *mut u8;
        std::ptr::copy(extradata.as_ptr(), codecpar.extradata, extradata.len());
        codecpar.extradata_size = extradata.len() as i32;
    };
}

pub(crate) trait StreamMutExt {
    fn update_codecpar<F: FnOnce(&mut AVCodecParameters)>(&mut self, func: F);
}

impl StreamMutExt for StreamMut<'_> {
    fn update_codecpar<F: FnOnce(&mut AVCodecParameters)>(&mut self, func: F) {
        let codecpar = unsafe { &mut *(*self.as_mut_ptr()).codecpar };
        func(codecpar);
    }
}
