use std::sync::Arc;

use smelter_render::Frame;

use crate::{
    pipeline::decoder::{
        EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
    },
    prelude::*,
};

pub enum QuickSyncH264Decoder {}

impl VideoDecoder for QuickSyncH264Decoder {
    const LABEL: &'static str = "Intel Quick Sync H264 decoder";

    fn new(
        _ctx: &Arc<PipelineCtx>,
        _keyframe_request_sender: Option<KeyframeRequestSender>,
    ) -> Result<Self, DecoderInitError> {
        Err(DecoderInitError::QuickSyncH264DecoderUnavailable(
            "support was not compiled into smelter-core".into(),
        ))
    }
}

impl VideoDecoderInstance for QuickSyncH264Decoder {
    fn decode(&mut self, _chunk: EncodedInputEvent) -> Vec<Frame> {
        match *self {}
    }

    fn flush(&mut self) -> Vec<Frame> {
        match *self {}
    }
}
