use std::{sync::Arc, time::Duration};

use crate::{
    error::InputInitError,
    pipeline::{
        input::hls::{HlsInput, HlsInputOptions},
        rtp::{RtpInput, RtpInputOptions},
        webrtc::{WhipInput, WhipInputOptions},
    },
};

use compositor_render::InputId;

use self::mp4::{Mp4Input, Mp4Options};

use super::{decoder::DecodedDataReceiver, PipelineCtx, Port};

#[cfg(feature = "decklink")]
pub mod decklink;
pub mod hls;
pub mod mp4;
pub mod raw_data;

pub enum Input {
    Rtp(RtpInput),
    Mp4(Mp4Input),
    Whip(WhipInput),
    Hls(HlsInput),
    #[cfg(feature = "decklink")]
    DeckLink(decklink::DeckLink),
    RawDataInput,
}

#[derive(Debug, Clone)]
pub enum InputOptions {
    Rtp(RtpInputOptions),
    Mp4(Mp4Options),
    Hls(HlsInputOptions),
    Whip(WhipInputOptions),
    #[cfg(feature = "decklink")]
    DeckLink(decklink::DeckLinkOptions),
}

pub enum InputInitInfo {
    Rtp {
        port: Option<Port>,
    },
    Mp4 {
        video_duration: Option<Duration>,
        audio_duration: Option<Duration>,
    },
    Whip {
        bearer_token: Arc<str>,
    },
    Other,
}

pub(super) fn new_external_input(
    ctx: Arc<PipelineCtx>,
    input_id: InputId,
    options: InputOptions,
) -> Result<(Input, InputInitInfo, DecodedDataReceiver), InputInitError> {
    match options {
        InputOptions::Rtp(opts) => RtpInput::new_input(ctx, input_id, opts),
        InputOptions::Mp4(opts) => Mp4Input::new_input(ctx, input_id, opts),
        InputOptions::Hls(opts) => HlsInput::new_input(ctx, input_id, opts),
        InputOptions::Whip(opts) => WhipInput::new_input(ctx, input_id, opts),
        #[cfg(feature = "decklink")]
        InputOptions::DeckLink(opts) => decklink::DeckLink::new_input(ctx, input_id, opts),
    }
}
