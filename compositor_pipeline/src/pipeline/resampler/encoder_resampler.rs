use compositor_render::error::ErrorStack;
use tracing::warn;

use crate::{
    audio_mixer::OutputSamples,
    pipeline::resampler::dynamic_resampler::{DynamicResampler, DynamicResamplerBatch},
    queue::PipelineEvent,
};

pub(crate) struct ResampledForEncoderStream<Source: Iterator<Item = PipelineEvent<OutputSamples>>> {
    resampler: DynamicResampler,
    input_sample_rate: u32,
    source: Source,
    eos_sent: bool,
}

impl<Source: Iterator<Item = PipelineEvent<OutputSamples>>> ResampledForEncoderStream<Source> {
    pub fn new(source: Source, input_sample_rate: u32, output_sample_rate: u32) -> Self {
        Self {
            input_sample_rate,
            resampler: DynamicResampler::new(output_sample_rate),
            source,
            eos_sent: false,
        }
    }
}

impl<Source: Iterator<Item = PipelineEvent<OutputSamples>>> Iterator
    for ResampledForEncoderStream<Source>
{
    type Item = Vec<PipelineEvent<OutputSamples>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let resampled = self
                    .resampler
                    .resample(from_output_samples(samples, self.input_sample_rate));
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
                        .map(|batch| PipelineEvent::Data(into_output_samples(batch)))
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

fn from_output_samples(value: OutputSamples, sample_rate: u32) -> DynamicResamplerBatch {
    DynamicResamplerBatch {
        samples: value.samples,
        start_pts: value.start_pts,
        sample_rate,
    }
}

fn into_output_samples(value: DynamicResamplerBatch) -> OutputSamples {
    OutputSamples {
        samples: value.samples,
        start_pts: value.start_pts,
    }
}
