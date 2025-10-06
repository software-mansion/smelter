use smelter_render::error::ErrorStack;
use std::iter;
use std::sync::Arc;
use tracing::warn;

use smelter_render::Frame;

use crate::pipeline::decoder::{AudioDecoder, DecodedSamples, VideoDecoder};
use crate::prelude::*;

pub(crate) struct VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    decoder: Decoder,
    source: Source,
    eos_sent: bool,
}

impl<Decoder, Source> VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    pub fn new(ctx: Arc<PipelineCtx>, source: Source) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx)?;
        Ok(Self {
            decoder,
            source,
            eos_sent: false,
        })
    }
}

impl<Decoder, Source> Iterator for VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    type Item = Vec<PipelineEvent<Frame>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let chunks = self.decoder.decode(samples);
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self.decoder.flush();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}

pub(crate) struct AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    decoder: Decoder,
    source: Source,
    eos_sent: bool,
}

impl<Decoder, Source> AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Decoder::Options,
        source: Source,
    ) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx, options)?;
        Ok(Self {
            decoder,
            source,
            eos_sent: false,
        })
    }
}

impl<Decoder, Source> Iterator for AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    type Item = Vec<PipelineEvent<DecodedSamples>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let result = self.decoder.decode(samples);
                let chunks = match result {
                    Ok(chunks) => chunks,
                    Err(err) => {
                        warn!(
                            "Audio decoder error: {}",
                            ErrorStack::new(&err).into_string()
                        );
                        return Some(vec![]);
                    }
                };
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self.decoder.flush();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}
