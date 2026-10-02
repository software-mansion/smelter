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

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn push_back_silence(&mut self, sample_count: usize) {
        self.buffer.push_back((self.silence(sample_count), 0));
    }

    pub fn push_front_silence(&mut self, sample_count: usize) {
        self.buffer.push_front((self.silence(sample_count), 0));
    }

    /// Fade in `len` samples starting at `start`.
    pub fn fade_in(&mut self, start: usize, len: usize) {
        let len = usize::min(len, self.frames().saturating_sub(start));
        self.for_each_sample_mut(start, len, |i, sample| *sample *= fade_gain(i, len));
    }

    /// Fade out the last `len` samples.
    pub fn fade_out_back(&mut self, len: usize) {
        let frames = self.frames();
        let len = usize::min(len, frames);
        self.for_each_sample_mut(frames - len, len, |i, sample| {
            *sample *= 1.0 - fade_gain(i, len)
        });
    }

    /// Drop first `sample_count` samples, crossfading the dropped samples into the remaining
    /// ones over `len` samples.
    pub fn crossfade_drain(&mut self, sample_count: usize, len: usize) {
        let len = usize::min(len, sample_count);
        let dropped = match self.read_samples(len) {
            AudioSamples::Mono(samples) => samples,
            AudioSamples::Stereo(samples) => samples.into_iter().flat_map(|(l, r)| [l, r]).collect(),
        };
        self.drain_samples(sample_count - len);

        let len = usize::min(len, self.frames());
        let mut dropped = dropped.into_iter();
        self.for_each_sample_mut(0, len, |i, sample| {
            let gain = fade_gain(i, len);
            *sample = dropped.next().unwrap_or(0.0) * (1.0 - gain) + *sample * gain;
        });
    }

    /// Call `f(i, sample)` for every channel sample of frames `start..start + len`, where `i`
    /// is the frame index relative to `start`.
    fn for_each_sample_mut(&mut self, start: usize, len: usize, mut f: impl FnMut(usize, &mut f64)) {
        let mut batch_start = 0;
        for (batch, read_samples) in &mut self.buffer {
            let batch_frames = batch.len() - *read_samples;
            let from = usize::max(start, batch_start);
            let to = usize::min(start + len, batch_start + batch_frames);
            for frame in from..to {
                let index = *read_samples + frame - batch_start;
                match batch {
                    AudioSamples::Mono(samples) => f(frame - start, &mut samples[index]),
                    AudioSamples::Stereo(samples) => {
                        f(frame - start, &mut samples[index].0);
                        f(frame - start, &mut samples[index].1);
                    }
                }
            }
            batch_start += batch_frames;
            if batch_start >= start + len {
                break;
            }
        }
    }

    fn silence(&self, sample_count: usize) -> AudioSamples {
        match self.channels {
            AudioChannels::Mono => AudioSamples::Mono(vec![0.0; sample_count]),
            AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); sample_count]),
        }
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

/// Raised cosine gain for sample `i` of a `len` samples long fade in.
fn fade_gain(i: usize, len: usize) -> f64 {
    0.5 - 0.5 * f64::cos(std::f64::consts::PI * (i as f64 + 0.5) / len as f64)
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
