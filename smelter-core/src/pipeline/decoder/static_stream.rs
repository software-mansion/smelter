use std::sync::Arc;
use tracing::warn;

use smelter_render::error::ErrorStack;

use crate::pipeline::{
    decoder::{
        AudioDecoder, DecodeOnlyFilter, DecoderStatsSender, EncodedInputEvent, VideoDecoder,
    },
    utils::input_sync::TrackKind,
};

use crate::prelude::*;

pub(crate) struct VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    decoder: Decoder,
    decode_only_filter: DecodeOnlyFilter,
    stats: DecoderStatsSender,
    source: Source,
}

impl<Decoder, Source> VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        source: Source,
    ) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx, None)?;
        Ok(Self {
            decoder,
            decode_only_filter: DecodeOnlyFilter::default(),
            stats: DecoderStatsSender::new(&ctx, input_ref, TrackKind::Video),
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
                self.stats.on_event(&event);
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
    /// Audio decoders do not reorder, so output of a decode-only chunk is dropped directly.
    last_chunk_decode_only: bool,
    stats: DecoderStatsSender,
    source: Source,
}

impl<Decoder, Source> AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedInputEvent>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: Decoder::Options,
        source: Source,
    ) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx, options)?;
        Ok(Self {
            decoder,
            last_chunk_decode_only: false,
            stats: DecoderStatsSender::new(&ctx, input_ref, TrackKind::Audio),
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
                self.stats.on_event(&event);
                if let EncodedInputEvent::Chunk(chunk) = &event {
                    self.last_chunk_decode_only = chunk.decode_only;
                }
                match self.decoder.decode(event) {
                    Ok(_) if self.last_chunk_decode_only => Some(vec![]),
                    Ok(samples) => Some(samples),
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
                let samples = self.decoder.flush();
                match samples.is_empty() || self.last_chunk_decode_only {
                    false => Some(samples),
                    true => None,
                }
            }
        }
    }
}
