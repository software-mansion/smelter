use crate::{
    audio_mixer::{AudioChannels, AudioSamples, OutputSamples},
    pipeline::resampler::dynamic_resampler::{DynamicStereoResampler, StereoSampleBatch},
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
                let channels = match &samples.samples {
                    AudioSamples::Mono(_) => AudioChannels::Mono,
                    AudioSamples::Stereo(_) => AudioChannels::Stereo,
                };
                let resampled = self
                    .resampler
                    .resample(from_output_sample(samples, self.input_sample_rate));
                Some(
                    resampled
                        .into_iter()
                        .flatten()
                        .map(|batch| PipelineEvent::Data(from_stereo_samples(batch, channels)))
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

fn from_output_sample(value: OutputSamples, sample_rate: u32) -> StereoSampleBatch {
    StereoSampleBatch {
        samples: match value.samples {
            AudioSamples::Mono(items) => {
                let channel: Vec<_> = items
                    .into_iter()
                    .map(|value| value as f64 / i16::MAX as f64)
                    .collect();
                (channel.clone(), channel)
            }
            AudioSamples::Stereo(items) => (
                items
                    .iter()
                    .map(|(l, _)| *l as f64 / i16::MAX as f64)
                    .collect(),
                items
                    .iter()
                    .map(|(_, r)| *r as f64 / i16::MAX as f64)
                    .collect(),
            ),
        },
        start_pts: value.start_pts,
        sample_rate,
    }
}

fn from_stereo_samples(value: StereoSampleBatch, channels: AudioChannels) -> OutputSamples {
    OutputSamples {
        samples: match channels {
            AudioChannels::Mono => AudioSamples::Mono(
                value
                    .samples
                    .0
                    .into_iter()
                    .map(|value| (value * i16::MAX as f64) as i16)
                    .collect(),
            ),
            AudioChannels::Stereo => AudioSamples::Stereo(
                value
                    .samples
                    .0
                    .into_iter()
                    .zip(value.samples.1.into_iter())
                    .map(|(l, r)| ((l * i16::MAX as f64) as i16, (r * i16::MAX as f64) as i16))
                    .collect(),
            ),
        },
        start_pts: value.start_pts,
    }
}
