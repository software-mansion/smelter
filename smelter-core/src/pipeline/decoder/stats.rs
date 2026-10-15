use std::sync::Arc;

use crate::{
    pipeline::{decoder::EncodedInputEvent, utils::input_sync::TrackKind},
    stats::{InputMediaStatsEvent, InputStatsEvent, StatsEvent, StatsSender},
};

use crate::prelude::*;

/// Reports encoded chunks entering a decoder to the stats module.
pub(super) struct DecoderStatsSender {
    input_ref: Ref<InputId>,
    track: TrackKind,
    stats_sender: StatsSender,
}

impl DecoderStatsSender {
    pub fn new(ctx: &Arc<PipelineCtx>, input_ref: Ref<InputId>, track: TrackKind) -> Self {
        Self {
            input_ref,
            track,
            stats_sender: ctx.stats_sender.clone(),
        }
    }

    pub fn on_event(&self, event: &EncodedInputEvent) {
        let EncodedInputEvent::Chunk(chunk) = event else {
            return;
        };
        if chunk.decode_only {
            return;
        }
        self.stats_sender.send(StatsEvent::Input {
            input_ref: self.input_ref.clone(),
            event: InputStatsEvent::Media {
                track: self.track,
                event: InputMediaStatsEvent::ChunkReceived {
                    size: chunk.data.len(),
                    pts: chunk.pts,
                },
            },
        });
    }
}
