use std::sync::Arc;
use tracing::warn;

use smelter_render::{Frame, error::ErrorStack};

use crate::pipeline::decoder::{AudioDecoder, EncodedInputEvent, VideoDecoder};

use crate::prelude::*;

pub(crate) struct VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    decoder: Decoder,
    source: Source,
}

impl<Decoder, Source> VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    pub fn new(ctx: Arc<PipelineCtx>, source: Source) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx, None)?;
        Ok(Self { decoder, source })
    }
}

impl<Decoder, Source> Iterator for VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    type Item = Vec<Frame>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(event)) => Some(self.decoder.decode(event)),
            Some(PipelineEvent::EOS) | None => {
                let chunks = self.decoder.flush();
                match chunks.is_empty() {
                    false => Some(chunks),
                    true => None,
                }
            }
        }
    }
}

pub(crate) struct AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    decoder: Decoder,
    source: Source,
}

impl<Decoder, Source> AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Decoder::Options,
        source: Source,
    ) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx, options)?;
        Ok(Self { decoder, source })
    }
}

impl<Decoder, Source> Iterator for AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    type Item = Vec<InputAudioSamples>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(event)) => match self.decoder.decode(event) {
                Ok(chunks) => Some(chunks),
                Err(err) => {
                    warn!(
                        "Audio decoder error: {}",
                        ErrorStack::new(&err).into_string()
                    );
                    Some(vec![])
                }
            },
            Some(PipelineEvent::EOS) | None => {
                let chunks = self.decoder.flush();
                match chunks.is_empty() {
                    false => Some(chunks),
                    true => None,
                }
            }
        }
    }
}
