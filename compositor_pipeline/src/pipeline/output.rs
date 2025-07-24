use std::sync::Arc;

use compositor_render::{Frame, OutputFrameFormat, OutputId, Resolution};
use crossbeam_channel::Sender;
use mp4::{Mp4Output, Mp4OutputOptions};
use rtmp::{RtmpClientOutput, RtmpSenderOptions};

use crate::{
    audio_mixer::OutputSamples,
    error::OutputInitError,
    pipeline::{
        output::hls::{HlsOutput, HlsOutputOptions},
        rtp::{RtpOutput, RtpOutputOptions},
    },
    queue::PipelineEvent,
};

use super::{
    encoder::{AudioEncoderOptions, VideoEncoderOptions},
    PipelineCtx, Port,
};
use whip::WhipSenderOptions;

pub mod encoded_data;
pub mod hls;
pub mod mp4;
pub mod raw_data;
pub mod rtmp;
pub mod whip;

#[derive(Debug, Clone)]
pub enum OutputOptions {
    Rtp(RtpOutputOptions),
    Rtmp(RtmpSenderOptions),
    Mp4(Mp4OutputOptions),
    Hls(HlsOutputOptions),
    Whip(WhipSenderOptions),
}

/// Options to configure output that sends h264 and opus audio via channel
#[derive(Debug, Clone)]
pub struct EncodedDataOutputOptions {
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

/// Options to configure output that sends raw PCM audio + wgpu textures via channel
#[derive(Debug, Clone)]
pub struct RawDataOutputOptions {
    pub video: Option<RawVideoOptions>,
    pub audio: Option<RawAudioOptions>,
}

/// Options to configure audio output that returns raw video via channel.
///
/// TODO: add option, for now it implies RGBA wgpu::Texture
#[derive(Debug, Clone)]
pub struct RawVideoOptions {
    pub resolution: Resolution,
}

/// Options to configure audio output that returns raw audio via channel.
///
/// TODO: add option, for now it implies 16-bit stereo
#[derive(Debug, Clone)]
pub struct RawAudioOptions;

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputVideo<'a> {
    pub resolution: Resolution,
    pub frame_format: OutputFrameFormat,
    pub frame_sender: &'a Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: &'a Sender<()>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputAudio<'a> {
    pub samples_batch_sender: &'a Sender<PipelineEvent<OutputSamples>>,
}

#[derive(Debug, Clone, Copy)]
pub enum OutputKind {
    Rtp,
    Rtmp,
    Whip,
    Mp4,
    Hls,
    EncodedDataChannel,
    RawDataChannel,
}

pub(crate) trait Output: Send {
    fn audio(&self) -> Option<OutputAudio>;
    fn video(&self) -> Option<OutputVideo>;
    fn kind(&self) -> OutputKind;
}

pub(super) fn new_external_output(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    options: OutputOptions,
) -> Result<(Box<dyn Output>, Option<Port>), OutputInitError> {
    match options {
        OutputOptions::Rtp(opt) => {
            let (output, port) = RtpOutput::new(ctx, output_id, opt)?;
            Ok((Box::new(output), Some(port)))
        }
        OutputOptions::Rtmp(opt) => {
            let output = RtmpClientOutput::new(ctx, output_id, opt)?;
            Ok((Box::new(output), None))
        }
        OutputOptions::Mp4(opt) => {
            let output = Mp4Output::new(ctx, output_id, opt)?;
            Ok((Box::new(output), None))
        }
        OutputOptions::Hls(opt) => {
            let output = HlsOutput::new(ctx, output_id, opt)?;
            Ok((Box::new(output), None))
        }
        OutputOptions::Whip(opt) => {
            let output = whip::WhipClientOutput::new(ctx, output_id, opt)?;
            Ok((Box::new(output), None))
        }
    }
}
