use crate::{
    audio_mixer::InputSamples,
    pipeline::{resampler::dynamic_resampler::DynamicStereoResampler, types::DecodedSamples},
    queue::PipelineEvent,
};

pub(crate) struct ResampledDecoderStream<Source: Iterator<Item = PipelineEvent<DecodedSamples>>> {
    resampler: DynamicStereoResampler,
    source: Source,
    eos_sent: bool,
}

impl<Source: Iterator<Item = PipelineEvent<DecodedSamples>>> ResampledDecoderStream<Source> {
    pub fn new(source: Source, output_sample_rate: u32) -> Self {
        Self {
            resampler: DynamicStereoResampler::new(output_sample_rate),
            source,
            eos_sent: false,
        }
    }
}

impl<Source: Iterator<Item = PipelineEvent<DecodedSamples>>> Iterator
    for ResampledDecoderStream<Source>
{
    type Item = Vec<PipelineEvent<InputSamples>>;

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
