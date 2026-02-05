use std::{collections::VecDeque, time::Duration};

use audioadapter::{Adapter, AdapterMut};
use rubato::{FixedSync, Resampler};
use tracing::{error, info, trace};

use crate::{AudioChannels, AudioSamples, PipelineEvent, prelude::OutputAudioSamples};

pub(crate) struct ResampledForEncoderStream<
    Source: Iterator<Item = PipelineEvent<OutputAudioSamples>>,
> {
    resampler: OutputResampler,
    source: Source,
    eos_sent: bool,
}

impl<Source: Iterator<Item = PipelineEvent<OutputAudioSamples>>> ResampledForEncoderStream<Source> {
    pub fn new(
        source: Source,
        input_sample_rate: u32,
        output_sample_rate: u32,
        channels: AudioChannels,
    ) -> Result<Self, rubato::ResamplerConstructionError> {
        let resampler = OutputResampler::new(input_sample_rate, output_sample_rate, channels)?;
        Ok(Self {
            resampler,
            source,
            eos_sent: false,
        })
    }
}

impl<Source: Iterator<Item = PipelineEvent<OutputAudioSamples>>> Iterator
    for ResampledForEncoderStream<Source>
{
    type Item = Vec<PipelineEvent<OutputAudioSamples>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                let resampled = self.resampler.resample(samples);
                Some(
                    resampled
                        .into_iter()
                        .map(|batch| PipelineEvent::Data(batch))
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

// This resampler assumes that:
// - It will receive continuous streams of data, without any gaps.
// - It can produce chunks of any size, no downstream components require specific sizes.
struct OutputResampler {
    output_sample_rate: u32,

    resampler_input_buffer: ResamplerInputBuffer,
    resampler_output_buffer: ResamplerOutputBuffer,
    resampler: rubato::Fft<f64>,

    /// Audio mixer guarantees continuity on output, so we only need to keep track
    /// of produces samples and first pts.
    first_sample_pts: Option<Duration>,
    samples_produced: u64,
}

impl OutputResampler {
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        channels: AudioChannels,
    ) -> Result<Self, rubato::ResamplerConstructionError> {
        info!(
            ?input_sample_rate,
            ?output_sample_rate,
            ?channels,
            "Create output resampler"
        );
        let samples_in_batch = 256;

        let resampler = rubato::Fft::<f64>::new(
            input_sample_rate as usize,
            output_sample_rate as usize,
            samples_in_batch,
            1,
            match channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            },
            FixedSync::Output,
        )?;
        // resampler delay expressed as time
        let output_delay = resampler.output_delay();

        let mut resampler_output_buffer = ResamplerOutputBuffer::new(channels, samples_in_batch);
        resampler_output_buffer.samples_to_drop = output_delay;

        Ok(Self {
            output_sample_rate,

            resampler,
            resampler_input_buffer: ResamplerInputBuffer::new(channels),
            resampler_output_buffer,

            first_sample_pts: None,
            samples_produced: 0,
        })
    }

    pub fn resample(&mut self, batch: OutputAudioSamples) -> Vec<OutputAudioSamples> {
        trace!(?batch, "Resampler received a new batch");
        let first_sample_pts = *self.first_sample_pts.get_or_insert(batch.start_pts);

        self.resampler_input_buffer.push_back(batch.samples);

        let mut result = vec![];
        while let Some(samples) = self.inner_resample() {
            let start_pts = first_sample_pts
                + Duration::from_secs_f64(
                    self.samples_produced as f64 / self.output_sample_rate as f64,
                );
            self.samples_produced += samples.len() as u64;
            result.push(OutputAudioSamples { samples, start_pts })
        }
        result
    }

    fn inner_resample(&mut self) -> Option<AudioSamples> {
        if self.resampler.input_frames_next() < self.resampler_input_buffer.frames() {
            return None;
        }

        let (consumed_samples, generated_samples) = match self.resampler.process_into_buffer(
            &self.resampler_input_buffer,
            &mut self.resampler_output_buffer,
            None,
        ) {
            Ok(result) => result,
            Err(err) => {
                error!("Resampling error: {err}");
                self.resampler_output_buffer.fill_with(&0.0);
                (0, self.resampler_output_buffer.frames())
            }
        };

        self.resampler_input_buffer.drain_samples(consumed_samples);
        if generated_samples != self.resampler_output_buffer.frames() {
            error!(
                expected = self.resampler_output_buffer.frames(),
                actual = generated_samples,
                "Resampler generated wrong amount of samples"
            )
        }

        Some(self.resampler_output_buffer.get_samples())
    }
}

#[derive(Debug)]
struct ResamplerInputBuffer {
    /// oldest samples are at the front, newest at the back
    buffer: VecDeque<(AudioSamples, usize)>,
    channels: AudioChannels,
}

impl ResamplerInputBuffer {
    fn new(channels: AudioChannels) -> Self {
        Self {
            buffer: VecDeque::new(),
            channels,
        }
    }

    fn push_back(&mut self, batch: AudioSamples) {
        self.buffer.push_back((batch, 0));
    }

    fn drain_samples(&mut self, mut samples_to_read: usize) {
        while let Some((batch, read_samples)) = self.buffer.front()
            && batch.len() - read_samples <= samples_to_read
        {
            samples_to_read -= batch.len() - read_samples;
            self.buffer.pop_front();
        }

        if let Some((_batch, read_samples)) = self.buffer.front_mut() {
            *read_samples += samples_to_read;
        }
    }
}

impl Adapter<'_, f64> for ResamplerInputBuffer {
    unsafe fn read_sample_unchecked(&self, channel: usize, frame: usize) -> f64 {
        let mut samples_skipped: usize = 0;
        for (batch, read_samples) in &self.buffer {
            if batch.len() - read_samples <= frame - samples_skipped {
                samples_skipped += batch.len() - read_samples;
            } else {
                match batch {
                    AudioSamples::Mono(items) => {
                        if channel != 0 {
                            break;
                        }
                        return items[frame + read_samples - samples_skipped];
                    }
                    AudioSamples::Stereo(items) => match channel {
                        0 => return items[frame + read_samples - samples_skipped].0,
                        1 => return items[frame + read_samples - samples_skipped].1,
                        _ => {
                            break;
                        }
                    },
                }
            }
        }
        error!(?channel, ?frame, "Sample does not exists");
        0.0
    }

    fn channels(&self) -> usize {
        match self.channels {
            AudioChannels::Mono => 1,
            AudioChannels::Stereo => 2,
        }
    }

    fn frames(&self) -> usize {
        self.buffer
            .iter()
            .map(|(batch, read_samples)| batch.len() - read_samples)
            .sum()
    }
}

#[derive(Debug)]
struct ResamplerOutputBuffer {
    buffer: AudioSamples,

    // resampler introduces delay, this value will be non zero if we know that
    // next resample will produce samples that can be dropped.
    samples_to_drop: usize,
}

impl ResamplerOutputBuffer {
    fn new(channels: AudioChannels, size: usize) -> Self {
        Self {
            buffer: match channels {
                AudioChannels::Mono => AudioSamples::Mono(vec![0.0; size]),
                AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); size]),
            },
            samples_to_drop: 0,
        }
    }

    fn get_samples(&mut self) -> AudioSamples {
        if self.samples_to_drop == 0 {
            return self.buffer.clone();
        }
        let start = usize::min(self.samples_to_drop, self.buffer.len());
        self.samples_to_drop = 0;
        match &self.buffer {
            AudioSamples::Mono(samples) => AudioSamples::Mono(samples[start..].to_vec()),
            AudioSamples::Stereo(samples) => AudioSamples::Stereo(samples[start..].to_vec()),
        }
    }
}

impl AdapterMut<'_, f64> for ResamplerOutputBuffer {
    unsafe fn write_sample_unchecked(&mut self, channel: usize, frame: usize, value: &f64) -> bool {
        match &mut self.buffer {
            AudioSamples::Mono(samples) => {
                if channel != 0 {
                    error!(?channel, "Wrong channel count");
                } else {
                    samples[frame] = *value
                };
            }
            AudioSamples::Stereo(samples) => match channel {
                0 => samples[frame].0 = *value,
                1 => samples[frame].1 = *value,
                _ => {
                    error!(?channel, "Wrong channel count");
                }
            },
        };
        false
    }
}

impl Adapter<'_, f64> for ResamplerOutputBuffer {
    unsafe fn read_sample_unchecked(&self, channel: usize, frame: usize) -> f64 {
        match &self.buffer {
            AudioSamples::Mono(samples) => {
                if channel != 0 {
                    error!(?channel, "Wrong channel count");
                }
                samples[frame]
            }
            AudioSamples::Stereo(samples) => match channel {
                0 => samples[frame].0,
                1 => samples[frame].1,
                _ => {
                    error!(?channel, "Wrong channel count");
                    samples[frame].0
                }
            },
        }
    }

    fn channels(&self) -> usize {
        match &self.buffer {
            AudioSamples::Mono(_) => 1,
            AudioSamples::Stereo(_) => 2,
        }
    }

    fn frames(&self) -> usize {
        self.buffer.len()
    }
}
