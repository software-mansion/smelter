use std::sync::Arc;

use smelter_render::Frame;

use crate::{
    pipeline::encoder::{VideoEncoder, VideoEncoderConfig},
    prelude::*,
};

pub enum QuickSyncH264Encoder {}

impl VideoEncoder for QuickSyncH264Encoder {
    const LABEL: &'static str = "Intel Quick Sync H264 encoder";

    type Options = QuickSyncH264EncoderOptions;

    fn new(
        _ctx: &Arc<PipelineCtx>,
        _options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        Err(EncoderInitError::QuickSyncH264EncoderUnavailable(
            "support was not compiled into smelter-core".into(),
        ))
    }

    fn encode(
        &mut self,
        _frame: Frame,
        _force_keyframe: bool,
    ) -> Vec<EncodedOutputChunk> {
        match *self {}
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        match *self {}
    }
}
