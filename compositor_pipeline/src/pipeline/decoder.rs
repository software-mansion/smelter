use crate::{audio_mixer::InputSamples, queue::PipelineEvent};

use super::types::VideoDecoder;

use bytes::Bytes;
use compositor_render::Frame;
use crossbeam_channel::Receiver;

pub use audio::AacDecoderError;

mod audio;
mod video;

pub(super) use audio::start_audio_decoder_thread;
pub(super) use audio::start_audio_resampler_only_thread;
pub(super) use video::start_video_decoder_thread;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDecoderOptions {
    pub decoder: VideoDecoder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDecoderOptions {
    Opus,
    Aac(AacDecoderOptions),
}

#[derive(Debug)]
pub struct DecodedDataReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<InputSamples>>>,
}

/// [RFC 3640, section 3.3.5. Low Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.5)
/// [RFC 3640, section 3.3.6. High Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.6)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AacDepayloaderMode {
    LowBitrate,
    HighBitrate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AacDecoderOptions {
    pub depayloader_mode: Option<AacDepayloaderMode>,
    pub asc: Option<Bytes>,
}
