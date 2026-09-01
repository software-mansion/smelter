use std::sync::Arc;
use tracing::warn;

use smelter_render::{Frame, error::ErrorStack};

use crate::pipeline::decoder::{AudioDecoder, DecodeOnlyFilter, EncodedInputEvent, VideoDecoder};

use crate::prelude::*;

pub(crate) struct VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    decoder: Decoder,
    decode_only_filter: DecodeOnlyFilter,
    source: Source,
}

impl<Decoder, Source> VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    pub fn new(ctx: Arc<PipelineCtx>, source: Source) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx, None)?;
        Ok(Self {
            decoder,
            decode_only_filter: DecodeOnlyFilter::default(),
            source,
        })
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
            Some(PipelineEvent::Data(event)) => {
                self.decode_only_filter.on_event(&event);
                let mut frames = self.decoder.decode(event);
                frames.retain(|frame| !self.decode_only_filter.should_drop(frame.pts));
                Some(frames)
            }
            Some(PipelineEvent::EOS) | None => {
                let mut frames = self.decoder.flush();
                frames.retain(|frame| !self.decode_only_filter.should_drop(frame.pts));
                match frames.is_empty() {
                    false => Some(frames),
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
    decode_only_filter: DecodeOnlyFilter,
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
        Ok(Self {
            decoder,
            decode_only_filter: DecodeOnlyFilter::default(),
            source,
        })
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
            Some(PipelineEvent::Data(event)) => {
                self.decode_only_filter.on_event(&event);
                match self.decoder.decode(event) {
                    Ok(mut samples) => {
                        samples.retain(|samples| {
                            !self.decode_only_filter.should_drop(samples.start_pts)
                        });
                        Some(samples)
                    }
                    Err(err) => {
                        warn!(
                            "Audio decoder error: {}",
                            ErrorStack::new(&err).into_string()
                        );
                        Some(vec![])
                    }
                }
            }
            Some(PipelineEvent::EOS) | None => {
                let mut samples = self.decoder.flush();
                samples.retain(|samples| !self.decode_only_filter.should_drop(samples.start_pts));
                match samples.is_empty() {
                    false => Some(samples),
                    true => None,
                }
            }
        }
    }
}
