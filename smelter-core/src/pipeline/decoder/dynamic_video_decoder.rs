use std::{iter, sync::Arc};

use smelter_render::{error::ErrorStack, Frame};
use tracing::error;

use crate::pipeline::decoder::video_decoder_mapping::VideoDecoderMapping;
use crate::pipeline::decoder::{
    ffmpeg_h264::FfmpegH264Decoder, ffmpeg_vp8::FfmpegVp8Decoder, ffmpeg_vp9::FfmpegVp9Decoder,
    vulkan_h264::VulkanH264Decoder, VideoDecoder, VideoDecoderInstance,
};

use crate::prelude::*;

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
}

impl<Source> DynamicVideoDecoderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    pub(crate) fn new(
        ctx: Arc<PipelineCtx>,
        decoders_info: VideoDecoderMapping,
        source: Source,
    ) -> Self {
        Self {
            ctx,
            decoder: None,
            last_chunk_kind: None,
            source,
            eos_sent: false,
            decoders_info,
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
