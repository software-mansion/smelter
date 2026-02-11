use std::collections::VecDeque;

use audioadapter::Adapter;
use tracing::error;

use crate::prelude::*;

#[derive(Debug)]
pub(crate) struct AudioSamplesBuffer {
    /// oldest samples are at the front, newest at the back
    buffer: VecDeque<(AudioSamples, usize)>,
    channels: AudioChannels,
}

impl AudioSamplesBuffer {
    pub fn new(channels: AudioChannels) -> Self {
        Self {
            buffer: VecDeque::new(),
            channels,
        }
    }

    pub fn push_back(&mut self, batch: AudioSamples) {
        self.buffer.push_back((batch, 0));
    }

    pub fn push_front(&mut self, batch: AudioSamples) {
        self.buffer.push_front((batch, 0));
    }

    pub fn drain_samples(&mut self, mut samples_to_read: usize) {
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

    /// Read first n samples (removes them from the buffer). Result is padded with zeros if there is not enough.
    pub fn read_samples(&mut self, sample_count: usize) -> AudioSamples {
        let mut samples = match self.channels {
            AudioChannels::Mono => AudioSamples::Mono(Vec::with_capacity(sample_count)),
            AudioChannels::Stereo => AudioSamples::Stereo(Vec::with_capacity(sample_count)),
        };

        let mut samples_to_read = sample_count;
        while let Some((batch, read_samples)) = self.buffer.front()
            && batch.len() - read_samples <= samples_to_read
        {
            samples_to_read -= batch.len() - read_samples;
            let (batch, read_samples) = self.buffer.pop_front().unwrap();
            match (batch, &mut samples) {
                (AudioSamples::Mono(batch), AudioSamples::Mono(samples)) => {
                    samples.extend_from_slice(&batch[read_samples..])
                }
                (AudioSamples::Stereo(batch), AudioSamples::Stereo(samples)) => {
                    samples.extend_from_slice(&batch[read_samples..])
                }
                _ => {
                    error!("Wrong channel layout");
                }
            }
        }

        if let Some((batch, read_samples)) = self.buffer.front_mut() {
            let range = *read_samples..(*read_samples + samples_to_read);
            *read_samples += samples_to_read;
            match (batch, &mut samples) {
                (AudioSamples::Mono(batch), AudioSamples::Mono(samples)) => {
                    samples.extend_from_slice(&batch[range])
                }
                (AudioSamples::Stereo(batch), AudioSamples::Stereo(samples)) => {
                    samples.extend_from_slice(&batch[range])
                }
                _ => {
                    error!("Wrong channel layout");
                }
            }
        }

        // Fill with zero samples if there is not enough data
        let range = 0..(sample_count - samples.len());
        match &mut samples {
            AudioSamples::Mono(samples) => samples.extend(range.map(|_| 0.0)),
            AudioSamples::Stereo(samples) => samples.extend(range.map(|_| (0.0, 0.0))),
        };
        samples
    }
}

impl Adapter<'_, f64> for AudioSamplesBuffer {
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
