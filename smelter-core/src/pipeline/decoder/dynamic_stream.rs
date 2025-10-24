use std::{iter, sync::Arc};

use smelter_render::{Frame, error::ErrorStack};
use tracing::error;

use crate::pipeline::decoder::{
    VideoDecoder, VideoDecoderInstance, ffmpeg_h264::FfmpegH264Decoder,
    ffmpeg_vp8::FfmpegVp8Decoder, ffmpeg_vp9::FfmpegVp9Decoder, vulkan_h264::VulkanH264Decoder,
};

use crate::prelude::*;

pub(crate) enum KeyframeRequestSender {
    Async(tokio::sync::mpsc::Sender<()>),
    #[allow(dead_code)]
    Sync(crossbeam_channel::Sender<()>),
}

impl KeyframeRequestSender {
    pub fn new_async() -> (Self, tokio::sync::mpsc::Receiver<()>) {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        (Self::Async(sender), receiver)
    }

    #[allow(dead_code)]
    pub fn new_sync() -> (Self, crossbeam_channel::Receiver<()>) {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        (Self::Sync(sender), receiver)
    }

    pub fn send(&self) {
        match &self {
            KeyframeRequestSender::Async(sender) => {
                let _ = sender.try_send(());
            }
            KeyframeRequestSender::Sync(sender) => {
                let _ = sender.try_send(());
            }
        }
    }
}

pub(crate) struct DynamicVideoDecoderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    ctx: Arc<PipelineCtx>,
    decoder: Option<Box<dyn VideoDecoderInstance>>,
    last_chunk_kind: Option<MediaKind>,
    source: Source,
    eos_sent: bool,
    decoders_info: VideoDecoderMapping,
    keyframe_request_sender: KeyframeRequestSender,
}

impl<Source> DynamicVideoDecoderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    pub(crate) fn new(
        ctx: Arc<PipelineCtx>,
        decoders_info: VideoDecoderMapping,
        source: Source,
        keyframe_request_sender: KeyframeRequestSender,
    ) -> Self {
        Self {
            ctx,
            decoder: None,
            last_chunk_kind: None,
            source,
            eos_sent: false,
            decoders_info,
            keyframe_request_sender,
        }
    }

    fn ensure_decoder(&mut self, chunk_kind: MediaKind) {
        if self.last_chunk_kind == Some(chunk_kind) {
            return;
        }
        self.last_chunk_kind = Some(chunk_kind);
        let preferred_decoder = match chunk_kind {
            MediaKind::Video(VideoCodec::H264) => self.decoders_info.h264,
            MediaKind::Video(VideoCodec::Vp8) => self.decoders_info.vp8,
            MediaKind::Video(VideoCodec::Vp9) => self.decoders_info.vp9,
            MediaKind::Audio(_) => {
                error!("Found audio packet in video stream.");
                None
            }
        };
        let Some(preferred_decoder) = preferred_decoder else {
            error!("No matching decoder found");
            return;
        };
        let decoder = match self.create_decoder(preferred_decoder) {
            Ok(decoder) => decoder,
            Err(err) => {
                error!(
                    "Failed to instantiate a decoder {}",
                    ErrorStack::new(&err).into_string()
                );
                return;
            }
        };
        self.decoder = Some(decoder);
    }

    fn create_decoder(
        &self,
        decoder: VideoDecoderOptions,
    ) -> Result<Box<dyn VideoDecoderInstance>, DecoderInitError> {
        let decoder: Box<dyn VideoDecoderInstance> = match decoder {
            VideoDecoderOptions::FfmpegH264 => Box::new(FfmpegH264Decoder::new(&self.ctx)?),
            VideoDecoderOptions::FfmpegVp8 => Box::new(FfmpegVp8Decoder::new(&self.ctx)?),
            VideoDecoderOptions::FfmpegVp9 => Box::new(FfmpegVp9Decoder::new(&self.ctx)?),
            VideoDecoderOptions::VulkanH264 => Box::new(VulkanH264Decoder::new(&self.ctx)?),
        };
        Ok(decoder)
    }
}

impl<Source> Iterator for DynamicVideoDecoderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    type Item = Vec<PipelineEvent<Frame>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                // TODO: flush on decoder change
                self.ensure_decoder(samples.kind);
                let decoder = self.decoder.as_mut()?;
                let chunks = decoder.decode(samples);
                // TODO: add better detection
                if chunks.is_empty() {
                    self.keyframe_request_sender.send();
                }
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self
                        .decoder
                        .as_mut()
                        .map(|decoder| decoder.flush())
                        .unwrap_or_default();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VideoDecoderMapping {
    pub h264: Option<VideoDecoderOptions>,
    pub vp8: Option<VideoDecoderOptions>,
    pub vp9: Option<VideoDecoderOptions>,
}

impl VideoDecoderMapping {
    pub fn has_any_codec(&self) -> bool {
        self.h264.is_some() || self.vp8.is_some() || self.vp9.is_some()
    }
}
