use std::{collections::VecDeque, time::Duration, usize};

use audioadapter::{Adapter, AdapterMut};
use rubato::{
    FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use tracing::{debug, error, warn};

use crate::{
    AudioChannels, AudioSamples, audio_mixer::SAMPLE_BATCH_DURATION, prelude::InputAudioSamples,
};

pub(super) struct ChannelResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,

    /// Resampler input buffer
    resampler_input_buffer: ResamplerInputBuffer,
    /// Resampler output buffer
    resampler_output_buffer: ResamplerOutputBuffer,

    output_buffer: OutputBuffer,

    resampler: rubato::Async<f64>,
    default_output_delay: Duration,

    input_buffer_end_pts: Duration,
    last_batch_returned_end: Duration,
    first_batch_pts: Duration,

    output_samples_to_drop: usize,

    consumed_samples: u64,
    produced_samples: u64,
}

pub(super) struct SingleChannelBatch {
    pub start_pts: Duration,
    pub samples: Vec<f64>,
}

#[cfg(debug_assertions)]
const INTERPOLATION_PARAMS: SincInterpolationParameters = SincInterpolationParameters {
    sinc_len: 128,
    f_cutoff: 0.95,
    oversampling_factor: 128,
    interpolation: SincInterpolationType::Linear,
    window: WindowFunction::Blackman2,
};

//#[cfg(not(debug_assertions))]
//const INTERPOLATION_PARAMS: SincInterpolationParameters = SincInterpolationParameters {
//    sinc_len: 256,
//    f_cutoff: 0.95,
//    oversampling_factor: 128,
//    interpolation: SincInterpolationType::Cubic,
//    window: WindowFunction::Blackman2,
//};

const CONTINUITY_THRESHOLD: Duration = Duration::from_millis(80);
const SHIFT_THRESHOLD: Duration = Duration::from_millis(5);
const STRETCH_THRESHOLD: Duration = Duration::from_millis(200);

impl ChannelResampler {
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        channels: usize,
        first_batch_pts: Duration,
    ) -> Result<Box<Self>, rubato::ResamplerConstructionError> {
        let samples_in_batch = ((output_sample_rate as u128 * SAMPLE_BATCH_DURATION.as_nanos())
            / 1_000_000_000) as usize;

        if !(output_sample_rate as u128 * SAMPLE_BATCH_DURATION.as_nanos())
            .is_multiple_of(1_000_000_000)
        {
            warn!(
                "Resampler cannot produce exactly {SAMPLE_BATCH_DURATION:?} chunks at sample rate {output_sample_rate}."
            )
        }

        let resampler = rubato::Async::<f64>::new_sinc(
            output_sample_rate as f64 / input_sample_rate as f64,
            1.10,
            &INTERPOLATION_PARAMS,
            samples_in_batch,
            1,
            FixedAsync::Output,
        )?;
        // resampler delay expressed as time
        let default_output_delay =
            Duration::from_secs(resampler.output_delay() as f64 / input_sample_rate as f64);

        // Input buffer is preallocated, to push input samples and fill missing samples between them.
        // Reallocation happens per every output batch, due to drain from the begging,
        // but this shouldn't have a noticeable performance impact and reduce code complexity.
        // This could be done without allocations, but it would complicate this code substantially.
        let input_buffer = Vec::new();

        // Output buffer is preallocated to avoid allocating it on every output batch.
        let output_buffer = vec![0.0; samples_in_batch];

        Ok(Box::new(Self {
            input_sample_rate,
            output_sample_rate,

            resampler,
            resampler_input_buffer: input_buffer,
            resampler_output_buffer: output_buffer,
            output_buffer: OutputBuffer::new(channels),

            default_output_delay,
            input_buffer_end_pts: first_batch_pts,
            last_batch_returned_end: first_batch_pts,
            first_batch_pts,
            output_samples_to_drop: resampler.output_delay(),

            consumed_samples: 0,
            produced_samples: 0,
        }))
    }

    fn input_buffer_start_pts(&self) -> Duration {
        self.input_buffer_end_pts
            .saturating_sub(Duration::from_secs(
                self.resampler_input_buffer.frames() as f64 / self.input_sample_rate as f64,
            ))
    }

    pub fn write_batch(&mut self, batch: InputAudioSamples) {
        let (start_pts, end_pts) = batch.pts_range();

        if start_pts > self.input_buffer_end_pts + CONTINUITY_THRESHOLD {
            let gap_duration = start_pts.saturating_sub(self.input_buffer_end_pts);
            let zero_samples =
                f64::floor(gap_duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
            self.resampler_input_buffer
                .push_front(match self.resampler_input_buffer.channels() {
                    1 => AudioSamples::Mono(vec![0.0; zero_samples]),
                    2 => AudioSamples::Stereo(vec![(0.0, 0.0); zero_samples]),
                })
        }
        if start_pts + CONTINUITY_THRESHOLD < self.input_buffer_end_pts {
            return;
        }
        self.input_buffer_end_pts = end_pts;
        self.resampler_input_buffer.push_back(batch);
    }

    pub fn read(&mut self, pts_range: (Duration, Duration)) -> AudioSamples {
        // input buffer that
        //  - represents a range X
        //  - has samples that represents Duration Y
        //
        // we are getting request to provide samples in range Z
        //  - if X and Y are large enough for Z then just process chunk and return samples
        //  - if X and Y are not enough for Z process what you can and fill with zeros
        //  - if
        //
        //
        let batch_size = ((pts_range.1 - pts_range.0).as_secs_f64()
            * self.output_sample_rate as f64)
            .round() as usize;

        while self.output_buffer.frames() < batch_size {
            // Number of samples that need to be produced by resampler
            let missing_samples = batch_size - self.output_buffer.frames();

            let requested_start_pts = pts_range.0
                + Duration::from_secs_f64(
                    self.output_buffer.frames() as f64 / self.output_sample_rate as f64,
                );

            // PTS of the first timestamp that would be produced from resampler
            // if current input buffer was resampled. It takes into account
            // that something is already in the internal buffer.
            let input_start_pts = self
                .input_buffer_start_pts()
                .saturating_sub(self.default_output_delay);

            if input_start_pts > requested_start_pts + STRETCH_THRESHOLD {
                // write full buffer of zeros (go through resampler)
                // TODO: handle discontinuity

                let gap_duration = input_start_pts.saturating_sub(self.requested_start_pts);
                let zero_samples =
                    f64::floor(gap_duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
                self.resampler_input_buffer.push_front(
                    match self.resampler_input_buffer.channels() {
                        1 => AudioSamples::Mono(vec![0.0; zero_samples]),
                        2 => AudioSamples::Stereo(vec![(0.0, 0.0); zero_samples]),
                    },
                );
                self.resampler.set_resample_ratio_relative(1.0, true);
            } else if input_start_pts > requested_start_pts + SHIFT_THRESHOLD {
                // stretch
                let drift = input_start_pts.saturating_sub(requested_start_pts);
                self.resampler.set_resample_ratio_relative(
                    1.0 + (0.1 * drift.as_secs_f64() / STRETCH_THRESHOLD.as_secs_f64()),
                    true,
                );
            } else if input_start_pts + SHIFT_THRESHOLD > requested_start_pts {
                // no squashing/stretching
                self.resampler.set_resample_ratio_relative(1.0, true);
            } else if input_start_pts + STRETCH_THRESHOLD > requested_start_pts {
                // squash
                let drift = requested_start_pts.saturating_sub(input_start_pts);
                self.resampler.set_resample_ratio_relative(
                    1.0 - (0.1 * drift.as_secs_f64() / STRETCH_THRESHOLD.as_secs_f64()),
                    true,
                );
            } else {
                // drop data
                // TODO: handle discontinuity

                let duration_to_drop = requested_start_pts.saturating_sub(input_start_pts);
                let samples_to_drop = duration_to_drop.as_secs_f64() * self.input_sample_rate;
                debug!(?samples_to_drop, ?pts_range, "Dropping samples");
                self.resampler_input_buffer.drain_samples(samples_to_drop);
                self.resampler.set_resample_ratio_relative(1.0, true);
            }

            self.inner_read();
        }
        return self.output_buffer.read_chunk(batch_size);
    }

    fn resample(&mut self) {
        let is_partial_read =
            self.resampler.input_frames_next() > self.resampler_input_buffer.frames();

        let indexing = match is_partial_read {
            true => Some(Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: Some(self.resampler_input_buffer.frames()),
                active_channels_mask: None,
            }),
            false => None,
        };
        let (consumed_samples, generated_samples) = match self.resampler.process_into_buffer(
            &self.resampler_input_buffer,
            &mut self.resampler_output_buffer,
            indexing,
        ) {
            Ok(result) => result,
            Err(err) => {
                error!("Resampling error: {err}");
                self.resampler_output_buffer.fill_with(0.0);
                (0, self.resampler_output_buffer.frames())
            }
        };

        self.resampler_input_buffer.drain_samples(consumed_samples);
        if self.output_samples_to_drop > 0 {
            self.resampler_output_buffer
                .drain_samples(self.output_samples_to_drop);
            self.output_samples_to_drop = 0;
        }
        if is_partial_read {
            self.output_samples_to_drop = self.resampler.output_delay();
        }
        self.output_buffer
            .push_front(self.resampler_output_buffer.buffer.clone());
    }
}

struct ResamplerInputBuffer {
    /// oldest samples are at the front, newest at the back
    buffer: VecDeque<(AudioSamples, usize)>,
    channels: AudioChannels,
}

impl ResamplerInputBuffer {
    fn new(channels: usize) -> Self {
        Self {
            buffer: VecDeque::new(),
            channels,
        }
    }

    fn push_back(&mut self, batch: AudioSamples) {
        self.buffer.push_back((batch, 0));
    }

    fn push_front(&mut self, batch: AudioSamples) {
        self.buffer.push_front((batch, 0));
    }

    fn drain_samples(&mut self, mut sample_count: usize) {
        while let Some((batch, read_samples)) = self.buffer.front()
            && batch.samples.len() < sample_count - read_samples
        {
            sample_count -= batch.samples.len();
            self.buffer.pop_front();
        }

        if let Some((batch, read_samples)) = self.buffer.front_mut() {
            *read_samples += sample_count;
        }
    }
}

impl Adapter<'_, f64> for ResamplerInputBuffer {
    unsafe fn read_sample_unchecked(&self, channel: usize, frame: usize) -> f64 {
        let batch_iter = self.buffer.iter();

        let mut sample_count: usize = 0;
        for (batch, read_samples) in &self.buffer {
            if batch.len() + sample_count < frame {
                sample_count += batch.len() - read_samples;
            } else {
                match batch.samples {
                    AudioSamples::Mono(items) => {
                        if channel != 0 {
                            break;
                        }
                        return items[frame];
                    }
                    AudioSamples::Stereo(items) => match channel {
                        0 => return items[frame].0,
                        1 => return items[frame].1,
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
        self.channels
    }

    fn frames(&self) -> usize {
        self.buffer.iter().map(|batch| batch.len()).sum()
    }
}

struct ResamplerOutputBuffer {
    buffer: AudioSamples,
    channels: usize,
}

impl ResamplerOutputBuffer {
    fn new(channels: usize, size: usize) -> Self {
        Self {
            buffer: match channels {
                1 => AudioSamples::Mono(vec![0.0; size]),
                2 => AudioSamples::Mono(vec![(0.0, 0.0); size]),
            },
            channels,
        }
    }

    fn drain_samples(&mut self, mut sample_count: usize) {
        match &mut self.buffer {
            AudioSamples::Mono(samples) => samples.drain(0..sample_count),
            AudioSamples::Stereo(samples) => samples.drain(0..sample_count),
        }
    }
}

impl AdapterMut<'_, f64> for ResamplerOutputBuffer {
    unsafe fn write_sample_unchecked(&mut self, channel: usize, frame: usize, value: &f64) -> bool {
        match self.buffer {
            AudioSamples::Mono(samples) => {
                if channel != 0 {
                    error!(?channel, "Wrong channel count");
                } else {
                    samples[frame] = *value;
                }
            }
            AudioSamples::Stereo(samples) => match channel {
                0 => samples[frame].0 = *value,
                1 => samples[frame].1 = *value,
                _ => {
                    error!(?channel, "Wrong channel count");
                }
            },
        }
    }
}

impl Adapter<'_, f64> for ResamplerOutputBuffer {
    unsafe fn read_sample_unchecked(&self, channel: usize, frame: usize) -> f64 {
        match self.buffer {
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
        self.channels
    }

    fn frames(&self) -> usize {
        self.buffer.len()
    }
}

struct OutputBuffer {
    buffer: VecDeque<(AudioSamples, usize)>,
    channels: AudioChannels,
}

impl OutputBuffer {
    fn new(channels: AudioChannels) -> Self {
        Self {
            buffer: VecDeque::new(),
            channels,
        }
    }
    fn append(&mut self, batch: AudioSamples) {
        self.buffer.push_back((batch, 0));
    }

    fn frames(&self) -> usize {
        self.buffer
            .iter()
            .map(|(batch, bytes_read)| batch.len() - bytes_read)
            .sum()
    }

    /// pad with zero if there is not enough
    fn read_chunk(&mut self, sample_count: usize) -> AudioSamples {
        let samples = match self.channels {
            AudioChannels::Mono => AudioSamples::Mono(Vec::with_capacity(sample_count)),
            AudioChannels::Stereo => AudioSamples::Stereo(Vec::with_capacity(sample_count)),
        };

        let mut samples_to_read = sample_count;
        while let Some((batch, read_samples)) = self.buffer.front()
            && batch.samples.len() < samples_to_read - read_samples
        {
            samples_to_read -= batch.samples.len();
            let (batch, read_samples) = self.buffer.pop_front().unwrap();
            match (batch, samples) {
                (AudioSamples::Mono(batch), AudioSamples::Mono(samples)) => {
                    samples.extend_from_slice(batch[read_samples..])
                }
                (AudioSamples::Stereo(batch), AudioSamples::Stereo(samples)) => {
                    samples.extend_from_slice(batch[read_samples..])
                }
                _ => {
                    error!("Wrong channel layout");
                }
            }
        }

        if let Some((batch, read_samples)) = self.buffer.front_mut() {
            *read_samples += samples_to_read;
            let range = read_samples..read_samples + samples_to_read;
            match (batch, samples) {
                (AudioSamples::Mono(batch), AudioSamples::Mono(samples)) => {
                    samples.extend_from_slice(batch[range])
                }
                (AudioSamples::Stereo(batch), AudioSamples::Stereo(samples)) => {
                    samples.extend_from_slice(batch[range])
                }
                _ => {
                    error!("Wrong channel layout");
                }
            }
        }

        // fill with zero if channel layouts would mismatch
        let range = 0..sample_count - samples.len();
        match samples {
            AudioSamples::Mono(items) => items.extend(range.map(|_| 0.0)),
            AudioSamples::Stereo(items) => items.extend(range.map(|_| (0.0, 0.0))),
        }
    }
}
