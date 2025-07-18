use std::{slice, sync::Arc, time::Duration};

use crate::{
    error::InputInitError,
    pipeline::{
        input::hls::{HlsInput, HlsInputOptions},
        webrtc::{WhipInput, WhipInputOptions},
    },
};

use bytes::Bytes;
use compositor_render::InputId;
use ffmpeg_next::{ffi::AVStream, Stream};
use rtp::{RtpInput, RtpInputOptions};

use self::mp4::{Mp4Input, Mp4Options};

use super::{decoder::DecodedDataReceiver, PipelineCtx, Port};

#[cfg(feature = "decklink")]
pub mod decklink;
pub mod hls;
pub mod mp4;
pub mod raw_data;
pub mod rtp;

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

// TODO(noituri): Remove if I'll end up using it only in HLS
pub(super) fn extra_data_from_stream(stream: &Stream<'_>) -> Option<Bytes> {
    unsafe {
        let codecpar = (*stream.as_ptr()).codecpar;
        let size = (*codecpar).extradata_size;
        match size > 0 {
            true => Some(bytes::Bytes::copy_from_slice(slice::from_raw_parts(
                (*codecpar).extradata,
                size as usize,
            ))),
            false => None,
        }
    }
}
