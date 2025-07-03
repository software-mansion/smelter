use crate::{
    audio_mixer::OutputSamples, pipeline::resampler::dynamic_resampler::DynamicStereoResampler,
    queue::PipelineEvent,
};

pub(crate) struct ResampledEncoderStream<Source: Iterator<Item = PipelineEvent<OutputSamples>>> {
    resampler: DynamicStereoResampler,
    input_sample_rate: u32,
    source: Source,
    eos_sent: bool,
}

impl<Source: Iterator<Item = PipelineEvent<OutputSamples>>> ResampledEncoderStream<Source> {
    pub fn new(source: Source, input_sample_rate: u32, output_sample_rate: u32) -> Self {
        Self {
            input_sample_rate,
            resampler: DynamicStereoResampler::new(output_sample_rate),
            source,
            eos_sent: false,
        }
    }
}

impl<Source: Iterator<Item = PipelineEvent<OutputSamples>>> Iterator
    for ResampledEncoderStream<Source>
{
    type Item = Vec<PipelineEvent<OutputSamples>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let resampled = self.resampler.resample(samples);
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
