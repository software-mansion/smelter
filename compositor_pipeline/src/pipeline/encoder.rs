use std::{iter, sync::Arc};

use compositor_render::{Frame, OutputFrameFormat, Resolution};
use tokio::sync::watch;

use crate::prelude::*;

pub(crate) mod encoder_thread_audio;
pub(crate) mod encoder_thread_video;

pub mod fdk_aac;
pub mod ffmpeg_h264;
pub mod ffmpeg_vp8;
pub mod ffmpeg_vp9;
pub mod libopus;

mod ffmpeg_utils;

#[derive(Debug, Clone)]
pub(crate) struct VideoEncoderConfig {
    pub resolution: Resolution,
    pub output_format: OutputFrameFormat,
    pub extradata: Option<bytes::Bytes>,
}

pub(crate) trait VideoEncoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError>;
    fn encode(&mut self, frame: Frame, force_keyframe: bool) -> Vec<EncodedOutputChunk>;
    fn flush(&mut self) -> Vec<EncodedOutputChunk>;
}

#[derive(Debug)]
pub(crate) struct AudioEncoderConfig {
    pub extradata: Option<bytes::Bytes>,
}

pub(crate) trait AudioEncoder: Sized {
    const LABEL: &'static str;

    type Options: AudioEncoderOptionsExt + Send + 'static;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, AudioEncoderConfig), EncoderInitError>;
    fn encode(&mut self, samples: OutputAudioSamples) -> Vec<EncodedOutputChunk>;
    fn flush(&mut self) -> Vec<EncodedOutputChunk>;
    fn set_packet_loss(&mut self, packet_loss: i32);
}

pub(super) struct VideoEncoderStreamContext {
    pub keyframe_request_sender: crossbeam_channel::Sender<()>,
    pub config: VideoEncoderConfig,
}

pub(super) struct VideoEncoderStream<Encoder, Source>
where
    Encoder: VideoEncoder,
    Source: Iterator<Item = PipelineEvent<Frame>>,
{
    encoder: Encoder,
    source: Source,
    keyframe_request_receiver: crossbeam_channel::Receiver<()>,
    eos_sent: bool,
}

impl<Encoder, Source> VideoEncoderStream<Encoder, Source>
where
    Encoder: VideoEncoder,
    Source: Iterator<Item = PipelineEvent<Frame>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Encoder::Options,
        source: Source,
    ) -> Result<(Self, VideoEncoderStreamContext), EncoderInitError> {
        let (keyframe_request_sender, keyframe_request_receiver) = crossbeam_channel::unbounded();
        let (encoder, config) = Encoder::new(&ctx, options)?;
        Ok((
            Self {
                encoder,
                source,
                eos_sent: false,
                keyframe_request_receiver,
            },
            VideoEncoderStreamContext {
                keyframe_request_sender,
                config,
            },
        ))
    }

    fn has_keyframe_request(&self) -> bool {
        let mut has_keyframe_request = false;
        while self.keyframe_request_receiver.try_recv().is_ok() {
            has_keyframe_request = true;
        }
        has_keyframe_request
    }
}

impl<Encoder, Source> Iterator for VideoEncoderStream<Encoder, Source>
where
    Encoder: VideoEncoder,
    Source: Iterator<Item = PipelineEvent<Frame>>,
{
    type Item = Vec<PipelineEvent<EncodedOutputChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(frame)) => {
                let chunks = self.encoder.encode(frame, self.has_keyframe_request());
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self.encoder.flush();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}

pub(super) struct AudioEncoderStreamContext {
    pub packet_loss_sender: watch::Sender<i32>,
    pub config: AudioEncoderConfig,
}

pub(super) struct AudioEncoderStream<Encoder, Source>
where
    Encoder: AudioEncoder,
    Source: Iterator<Item = PipelineEvent<OutputAudioSamples>>,
{
    encoder: Encoder,
    source: Source,
    packet_loss_receiver: watch::Receiver<i32>,
    eos_sent: bool,
}

impl<Encoder, Source> AudioEncoderStream<Encoder, Source>
where
    Encoder: AudioEncoder,
    Source: Iterator<Item = PipelineEvent<OutputAudioSamples>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Encoder::Options,
        source: Source,
    ) -> Result<(Self, AudioEncoderStreamContext), EncoderInitError> {
        let (packet_loss_sender, packet_loss_receiver) = watch::channel(0);
        let (encoder, config) = Encoder::new(&ctx, options)?;

        Ok((
            Self {
                encoder,
                source,
                packet_loss_receiver,
                eos_sent: false,
            },
            AudioEncoderStreamContext {
                packet_loss_sender,
                config,
            },
        ))
    }

    fn updated_packet_loss(&mut self) -> Option<i32> {
        let packet_loss_changed = self.packet_loss_receiver.has_changed().unwrap_or(false);
        match packet_loss_changed {
            true => Some(*self.packet_loss_receiver.borrow_and_update()),
            false => None,
        }
    }
}

impl<Encoder, Source> Iterator for AudioEncoderStream<Encoder, Source>
where
    Encoder: AudioEncoder,
    Source: Iterator<Item = PipelineEvent<OutputAudioSamples>>,
{
    type Item = Vec<PipelineEvent<EncodedOutputChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                if let Some(packet_loss) = self.updated_packet_loss() {
                    self.encoder.set_packet_loss(packet_loss);
                }
                let chunks = self.encoder.encode(samples);
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self.encoder.flush();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}
