use std::sync::Arc;

use crate::{
    pipeline::{input::hls::HlsInput, mp4::Mp4Input, rtp::RtpInput, webrtc::WhipInput},
    queue::QueueDataReceiver,
};

use crate::prelude::*;

#[cfg(feature = "decklink")]
pub mod decklink;
pub mod hls;
pub mod raw_data;

pub enum Input {
    Rtp(RtpInput),
    Mp4(Mp4Input),
    Whip(WhipInput),
    Hls(HlsInput),
    #[cfg(feature = "decklink")]
    DeckLink(decklink::DeckLink),
    RawDataChannel,
}

impl Input {
    pub fn kind(&self) -> InputProtocolKind {
        match self {
            Input::Rtp(_input) => InputProtocolKind::Rtp,
            Input::Mp4(_input) => InputProtocolKind::Mp4,
            Input::Whip(_input) => InputProtocolKind::Whip,
            Input::Hls(_input) => InputProtocolKind::Hls,
            #[cfg(feature = "decklink")]
            Input::DeckLink(_input) => InputProtocolKind::DeckLink,
            Input::RawDataChannel => InputProtocolKind::RawDataChannel,
        }
    }
}

pub(super) fn new_external_input(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    options: ProtocolInputOptions,
) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
    match options {
        ProtocolInputOptions::Rtp(opts) => RtpInput::new_input(ctx, input_id, opts),
        ProtocolInputOptions::Mp4(opts) => Mp4Input::new_input(ctx, input_id, opts),
        ProtocolInputOptions::Hls(opts) => HlsInput::new_input(ctx, input_id, opts),
        ProtocolInputOptions::Whip(opts) => WhipInput::new_input(ctx, input_id, opts),
        #[cfg(feature = "decklink")]
        ProtocolInputOptions::DeckLink(opts) => decklink::DeckLink::new_input(ctx, input_id, opts),
    }
}
