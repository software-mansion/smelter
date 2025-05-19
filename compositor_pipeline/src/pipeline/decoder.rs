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

#[derive(Debug)]
pub struct DecodedDataReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<InputSamples>>>,
}

