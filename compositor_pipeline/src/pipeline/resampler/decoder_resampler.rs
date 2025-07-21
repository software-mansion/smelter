use std::{sync::Arc, time::Duration};

use compositor_render::error::ErrorStack;
use tracing::warn;

use crate::pipeline::{
    resampler::dynamic_resampler::{DynamicResampler, DynamicResamplerBatch},
    types::DecodedSamples,
};
use crate::prelude::*;

pub(crate) struct ResampledDecoderStream<Source: Iterator<Item = PipelineEvent<DecodedSamples>>> {
    resampler: DynamicResampler,
    source: Source,
    eos_sent: bool,
}

impl<Source: Iterator<Item = PipelineEvent<DecodedSamples>>> ResampledDecoderStream<Source> {
    pub fn new(output_sample_rate: u32, source: Source) -> Self {
        Self {
            resampler: DynamicResampler::new(output_sample_rate),
            source,
            eos_sent: false,
        }
    }
}

impl<Source: Iterator<Item = PipelineEvent<DecodedSamples>>> Iterator
    for ResampledDecoderStream<Source>
{
    type Item = Vec<PipelineEvent<InputAudioSamples>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let resampled = self.resampler.resample(from_decoded_samples(samples));
                let resampled = match resampled {
                    Ok(resampled) => resampled,
                    Err(err) => {
                        warn!("Resampler error: {}", ErrorStack::new(&err).into_string());
                        return Some(vec![]);
                    }
                };
                Some(
                    resampled
                        .into_iter()
                        .map(Into::into)
                        .map(PipelineEvent::Data)
                        .collect(),
                )
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

fn from_decoded_samples(value: DecodedSamples) -> DynamicResamplerBatch {
    DynamicResamplerBatch {
        samples: value.samples,
        start_pts: value.start_pts,
        sample_rate: value.sample_rate,
    }
}

impl From<DynamicResamplerBatch> for InputAudioSamples {
    fn from(value: DynamicResamplerBatch) -> Self {
        let end_pts = value.start_pts
            + Duration::from_secs_f64(
                value.samples.sample_count() as f64 / value.sample_rate as f64,
            );
        InputAudioSamples {
            samples: match value.samples {
                AudioSamples::Mono(samples) => {
                    Arc::new(samples.into_iter().map(|v| (v, v)).collect())
                }
                AudioSamples::Stereo(samples) => Arc::new(samples),
            },
            start_pts: value.start_pts,
            end_pts,
        }
    }
}
