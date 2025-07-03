use std::iter;
use std::sync::Arc;

use crate::error::{DecoderInitError, InputInitError};
use crate::pipeline::{EncodedChunk, PipelineCtx};
use crate::{audio_mixer::InputSamples, queue::PipelineEvent};

use super::types::VideoDecoder;

use bytes::Bytes;
use compositor_render::Frame;
use crossbeam_channel::Receiver;

pub use audio::AacDecoderError;

mod audio;
mod video;

mod decoder_thread_audio;
mod decoder_thread_video;

mod ffmpeg_h264;
mod ffmpeg_vp8;
mod ffmpeg_vp9;
mod vulkan_h264;

pub(super) use audio::start_audio_decoder_thread;
pub(super) use audio::start_audio_resampler_only_thread;
use ffmpeg_next::codec::traits::Decoder;
pub(super) use video::start_video_decoder_thread;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDecoderOptions {
    pub decoder: VideoDecoder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDecoderOptions {
    Opus(OpusDecoderOptions),
    Aac(AacDecoderOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpusDecoderOptions {
    pub forward_error_correction: bool,
}

#[derive(Debug)]
pub struct DecodedDataReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<InputSamples>>>,
}

/// [RFC 3640, section 3.3.5. Low Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.5)
/// [RFC 3640, section 3.3.6. High Bit-rate AAC](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.6)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AacDepayloaderMode {
    LowBitrate,
    HighBitrate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AacDecoderOptions {
    pub depayloader_mode: Option<AacDepayloaderMode>,
    pub asc: Option<Bytes>,
}

pub(crate) trait VideoDecoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError>;
    fn decode(&mut self, chunk: EncodedChunk) -> Vec<Frame>;
    fn flush(&mut self) -> Vec<Frame>;
}

pub(crate) trait AudioDecoder: Sized {
    const LABEL: &'static str;
    type Options: Send + 'static;

    fn new(ctx: &Arc<PipelineCtx>, options: Self::Options) -> Result<Self, DecoderInitError>;
    fn decode(&mut self, chunk: EncodedChunk) -> Vec<InputSamples>;
    fn flush(&mut self) -> Vec<InputSamples>;
}

pub(super) struct VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    decoder: Decoder,
    source: Source,
    eos_sent: bool,
}

impl<Decoder, Source> VideoDecoderStream<Decoder, Source>
where
    Decoder: VideoDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    pub fn new(
        ctx: Arc<PipelineCtx>,
        options: Decoder::Options,
        source: Source,
    ) -> Result<Self, DecoderInitError> {
        let decoder = Decoder::new(&ctx, options)?;
        Ok((Self {
            decoder,
            source,
            eos_sent: false,
        }))
    }
}

pub(super) struct AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    decoder: Decoder,
    source: Source,
    eos_sent: bool,
}

impl<Decoder, Source> AudioDecoderStream<Decoder, Source>
where
    Decoder: AudioDecoder,
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
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
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    type Item = Vec<PipelineEvent<EncodedChunk>>;

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

pub(super) struct ResampledStream<Source: Iterator<Item = PipelineEvent<OutputSamples>>> {
    resampler: Option<OutputResampler>,
    source: Source,
    eos_sent: bool,
}

impl<Source: Iterator<Item = PipelineEvent<OutputSamples>>> ResampledStream<Source> {
    pub fn new(
        source: Source,
        input_sample_rate: u32,
        output_sample_rate: u32,
    ) -> Result<Self, DecoderInitError> {
        let resampler = match input_sample_rate != output_sample_rate {
            true => Some(OutputResampler::new(input_sample_rate, output_sample_rate)?),
            false => None,
        };
        Ok(Self {
            resampler,
            source,
            eos_sent: false,
        })
    }
}

impl<Source: Iterator<Item = PipelineEvent<OutputSamples>>> Iterator for ResampledStream<Source> {
    type Item = Vec<PipelineEvent<OutputSamples>>;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(resampler) = &mut self.resampler else {
            return self.source.next().map(|event| vec![event]);
        };
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let resampled = resampler.resample(samples);
                Some(resampled.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    self.eos_sent = true;
                    Some(vec![PipelineEvent::EOS])
                }
            },
        }
    }
}
