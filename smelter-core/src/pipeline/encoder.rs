use std::{iter, sync::Arc, time::Duration};

use smelter_render::{ExternalNv12FramePool, Frame, OutputFrameFormat, Resolution};
use tokio::sync::watch;

use crate::prelude::*;

pub(crate) mod encoder_thread_audio;
pub(crate) mod encoder_thread_video;

pub mod fdk_aac;
pub mod ffmpeg_h264;
pub mod ffmpeg_vp8;
pub mod ffmpeg_vp9;
pub mod libopus;

#[cfg(all(feature = "quicksync", target_os = "linux"))]
pub mod quicksync_h264;

#[cfg(not(all(feature = "quicksync", target_os = "linux")))]
#[path = "./encoder/quicksync_h264_fallback.rs"]
pub mod quicksync_h264;

#[cfg(feature = "gpu-video")]
pub mod vulkan_h264;

#[cfg(not(feature = "gpu-video"))]
#[path = "./encoder/vulkan_h264_fallback.rs"]
pub mod vulkan_h264;

mod ffmpeg_utils;
pub(crate) mod resampler;
mod utils;

#[derive(Debug, Clone)]
pub(crate) struct VideoEncoderConfig {
    pub resolution: Resolution,
    pub output_format: OutputFrameFormat,
    pub extradata: Option<bytes::Bytes>,
    /// Encoder-owned dma-buf NV12 pool for the zero-copy "reverse ownership"
    /// path: when present, the compositor renders the NV12 output directly into
    /// these surfaces (set only by the Quick Sync encoder on its zero-copy path).
    pub external_nv12_pool: Option<Arc<dyn ExternalNv12FramePool>>,
}

pub(crate) trait VideoEncoder: Sized {
    const LABEL: &'static str;
    const OUTPUT_POLL_INTERVAL: Option<Duration> = None;
    type Options: Send + 'static;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError>;
    fn encode(&mut self, frame: Frame, force_keyframe: bool) -> Vec<EncodedOutputChunk>;
    fn poll_output(&mut self) -> Vec<EncodedOutputChunk> {
        Vec::new()
    }
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

pub(super) struct VideoEncoderStream<Encoder>
where
    Encoder: VideoEncoder,
{
    encoder: Encoder,
    source: crossbeam_channel::Receiver<PipelineEvent<Frame>>,
    keyframe_request_receiver: crossbeam_channel::Receiver<()>,
    eos_sent: bool,
}

impl<Encoder> VideoEncoderStream<Encoder>
where
    Encoder: VideoEncoder,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Encoder::Options,
        source: crossbeam_channel::Receiver<PipelineEvent<Frame>>,
    ) -> Result<(Self, VideoEncoderStreamContext), EncoderInitError> {
        let (keyframe_request_sender, keyframe_request_receiver) =
            crossbeam_channel::unbounded();
        let (encoder, config) = Encoder::new(&ctx, options)?;
        Ok((
            Self { encoder, source, eos_sent: false, keyframe_request_receiver },
            VideoEncoderStreamContext { keyframe_request_sender, config },
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

impl<Encoder> Iterator for VideoEncoderStream<Encoder>
where
    Encoder: VideoEncoder,
{
    type Item = Vec<PipelineEvent<EncodedOutputChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = match Encoder::OUTPUT_POLL_INTERVAL {
            Some(interval) => loop {
                match self.source.recv_timeout(interval) {
                    Ok(event) => break Some(event),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        let chunks = self.encoder.poll_output();
                        if !chunks.is_empty() {
                            return Some(
                                chunks.into_iter().map(PipelineEvent::Data).collect(),
                            );
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break None,
                }
            },
            None => self.source.recv().ok(),
        };

        match event {
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
            Self { encoder, source, packet_loss_receiver, eos_sent: false },
            AudioEncoderStreamContext { packet_loss_sender, config },
        ))
    }

    fn updated_packet_loss(&mut self) -> Option<i32> {
        let packet_loss_changed =
            self.packet_loss_receiver.has_changed().unwrap_or(false);
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
