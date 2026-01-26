use std::{collections::VecDeque, time::Duration};

use audioadapter::{Adapter, AdapterMut};
use rubato::{
    FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use tracing::{debug, error, trace, warn};

use crate::{AudioChannels, AudioSamples, prelude::InputAudioSamples};

/// Data flow:
/// Initial data is appended to `resampler_input_buffer`. When we need to get samples for specific
/// pts range, resampler is resampling data from `resampler_input_buffer` into `resampler_output_buffer`.
/// After every resample run, then content of `resampler_output_buffer` is appended to
/// `output_buffer`. When output_buffer have enough samples for the range, then data is returned.
///
/// Controlling variables:
/// There are 2 main factors that define whether we should stretch or squash data:
/// - Requested start_pts
///   - If `output_buffer` have some data we shift that start_pts by their duration.
/// - PTS of the first sample in `resampler_input_buffer`
///   - This value is calculated every time because we know number of samples and end pts of the
///     last written batch.
///
/// In particular this behavior might mean that we try to squash samples even if we don't have
/// enough data to resample full batch.
pub(super) struct InputResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: AudioChannels,

    /// Resampler input buffer
    resampler_input_buffer: ResamplerInputBuffer,
    /// Resampler output buffer
    resampler_output_buffer: ResamplerOutputBuffer,

    output_buffer: OutputBuffer,

    resampler: rubato::Async<f64>,
    original_output_delay: Duration,
    original_resampler_ratio: f64,

    input_buffer_end_pts: Duration,

    /// Should be set to false after first resample. Before first resample we are
    /// not attempting to stretch audio. `get_samples` should return zeros until
    /// we reach synchronization.
    before_first_resample: bool,
}

/// Should be on par with FFT resampler, but more CPU intensive.
/// It takes around 500µs to process 20ms chunk in Release.
pub(super) const SLOW_INTERPOLATION_PARAMS: SincInterpolationParameters =
    SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        oversampling_factor: 128,
        interpolation: SincInterpolationType::Cubic,
        window: WindowFunction::Blackman2,
    };

/// Fast interpolation, intended for Debug mode and when the sample rates
/// match. Quality here is less important because it only matters when stretching
/// or squashing audio.
///
/// It takes around 150µs to process 20ms chunk in Release mode and about 4ms in Debug.
pub(super) const FAST_INTERPOLATION_PARAMS: SincInterpolationParameters =
    SincInterpolationParameters {
        sinc_len: 32,
        f_cutoff: 0.95,
        oversampling_factor: 128,
        interpolation: SincInterpolationType::Linear,
        window: WindowFunction::Blackman2,
    };

const CONTINUITY_THRESHOLD: Duration = Duration::from_millis(80);
const SHIFT_THRESHOLD: Duration = Duration::from_millis(2);
const STRETCH_THRESHOLD: Duration = Duration::from_millis(400);

impl InputResampler {
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        channels: AudioChannels,
        first_batch_pts: Duration,
    ) -> Result<Self, rubato::ResamplerConstructionError> {
        debug!(
            ?input_sample_rate,
            ?output_sample_rate,
            ?channels,
            "Create input resampler"
        );
        let samples_in_batch = 256;

        let original_resampler_ratio = output_sample_rate as f64 / input_sample_rate as f64;
        let resampler = rubato::Async::<f64>::new_sinc(
            original_resampler_ratio,
            1.10,
            Self::interpolation_params(input_sample_rate, output_sample_rate),
            samples_in_batch,
            match channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            },
            FixedAsync::Output,
        )?;
        // resampler delay expressed as time
        let output_delay = resampler.output_delay();
        let default_output_delay =
            Duration::from_secs_f64(output_delay as f64 / input_sample_rate as f64);

        let mut resampler_output_buffer = ResamplerOutputBuffer::new(channels, samples_in_batch);
        resampler_output_buffer.samples_to_drop = output_delay;

        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            channels,

            resampler,
            resampler_input_buffer: ResamplerInputBuffer::new(channels),
            resampler_output_buffer,
            output_buffer: OutputBuffer::new(channels),

            original_output_delay: default_output_delay,
            original_resampler_ratio,
            input_buffer_end_pts: first_batch_pts,

            before_first_resample: true,
        })
    }

    fn interpolation_params(
        input_sample_rate: u32,
        output_sample_rate: u32,
    ) -> &'static SincInterpolationParameters {
        if input_sample_rate == output_sample_rate || cfg!(debug_assertions) {
            &FAST_INTERPOLATION_PARAMS
        } else {
            &SLOW_INTERPOLATION_PARAMS
        }
    }

    pub fn channels(&self) -> AudioChannels {
        self.channels
    }

    pub fn input_sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    fn input_buffer_start_pts(&self) -> Duration {
        self.input_buffer_end_pts
            .saturating_sub(Duration::from_secs_f64(
                self.resampler_input_buffer.frames() as f64 / self.input_sample_rate as f64,
            ))
    }

    fn set_resample_ratio_relative(&mut self, rel_ratio: f64) {
        let rel_ratio = rel_ratio.clamp(1.0 / 1.1, 1.1);
        let desired = self.original_resampler_ratio * rel_ratio;
        let current = self.resampler.resample_ratio();
        let should_update = (current == 1.0 && desired != 1.0) || (desired - current).abs() > 0.01;
        if should_update
            && let Err(err) = self.resampler.set_resample_ratio_relative(rel_ratio, true)
        {
            warn!(%err, "Failed to update resampler ratio.");
            let _ = self.resampler.set_resample_ratio_relative(1.0, true);
        }
    }

    /// Write new input batches for processing. Data is written to `resampler_input_buffer`
    /// as it is one after the other. If there is a discontinuity, this
    /// function will fill a gap with zeros or drop overlapping batches.
    pub fn write_batch(&mut self, batch: InputAudioSamples) {
        let (start_pts, end_pts) = batch.pts_range();
        trace!(
            ?start_pts,
            ?end_pts,
            len = batch.len(),
            "Resampler received a new batch"
        );

        if start_pts > self.input_buffer_end_pts + CONTINUITY_THRESHOLD {
            let gap_duration = start_pts.saturating_sub(self.input_buffer_end_pts);
            let zero_samples =
                f64::floor(gap_duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
            trace!(zero_samples, "Detected gap, filling with zero samples");
            let samples = match self.channels {
                AudioChannels::Mono => AudioSamples::Mono(vec![0.0; zero_samples]),
                AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); zero_samples]),
            };
            self.resampler_input_buffer.push_back(samples)
        }
        if start_pts + CONTINUITY_THRESHOLD < self.input_buffer_end_pts {
            trace!("Detected overlapping batches, dropping.");
            return;
        }
        self.input_buffer_end_pts = end_pts;

        self.resampler_input_buffer.push_back(batch.samples);
    }

    pub fn get_samples(&mut self, pts_range: (Duration, Duration)) -> AudioSamples {
        if let Some(zero_batch) = self.maybe_prepare_before_resample(pts_range) {
            return zero_batch;
        };

        let batch_size = ((pts_range.1 - pts_range.0).as_secs_f64()
            * self.output_sample_rate as f64)
            .round() as usize;

        while self.output_buffer.frames() < batch_size {
            let requested_start_pts = pts_range.0
                + Duration::from_secs_f64(
                    self.output_buffer.frames() as f64 / self.output_sample_rate as f64,
                );

            // PTS of the first timestamp that would be produced from resampler
            // if current input buffer was resampled. It takes into account
            // that something is already in the internal buffer.
            let input_start_pts = self
                .input_buffer_start_pts()
                .saturating_sub(self.original_output_delay);

            if input_start_pts > requested_start_pts + STRETCH_THRESHOLD {
                // write full buffer of zeros (go through resampler)
                // TODO: handle discontinuity

                let gap_duration = input_start_pts.saturating_sub(requested_start_pts);
                let zero_samples =
                    (gap_duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
                let samples = match self.channels {
                    AudioChannels::Mono => AudioSamples::Mono(vec![0.0; zero_samples]),
                    AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); zero_samples]),
                };
                self.resampler_input_buffer.push_front(samples);
                self.set_resample_ratio_relative(1.0);
                trace!(
                    zero_samples,
                    ?gap_duration,
                    "Input buffer behind, writing zeroes samples"
                )
            } else if input_start_pts > requested_start_pts + SHIFT_THRESHOLD {
                // stretch
                let drift = input_start_pts.saturating_sub(requested_start_pts);
                let ratio = drift.as_secs_f64() / STRETCH_THRESHOLD.as_secs_f64();
                self.set_resample_ratio_relative(1.0 + (0.1 * ratio));
                trace!(ratio, ?drift, "Input buffer behind, stretching");
            } else if input_start_pts + SHIFT_THRESHOLD > requested_start_pts {
                // no squashing/stretching
                self.set_resample_ratio_relative(1.0);
                trace!("Input buffer on time");
            } else if input_start_pts + STRETCH_THRESHOLD > requested_start_pts {
                // squash
                let drift = requested_start_pts.saturating_sub(input_start_pts);
                let ratio = drift.as_secs_f64() / STRETCH_THRESHOLD.as_secs_f64();
                self.set_resample_ratio_relative(1.0 - (0.1 * ratio));
                trace!(ratio, ?drift, "Input buffer ahead, squashing");
            } else {
                // drop data
                // TODO: handle discontinuity

                let duration_to_drop = requested_start_pts.saturating_sub(input_start_pts);
                let samples_to_drop =
                    (duration_to_drop.as_secs_f64() * self.input_sample_rate as f64) as usize;
                self.resampler_input_buffer.drain_samples(samples_to_drop);
                self.set_resample_ratio_relative(1.0);
                trace!(
                    samples_to_drop,
                    ?duration_to_drop,
                    "Input buffer ahead, dropping samples"
                );
            }

            self.resample();
        }
        self.output_buffer.read_chunk(batch_size)
    }

    fn maybe_prepare_before_resample(
        &mut self,
        pts_range: (Duration, Duration),
    ) -> Option<AudioSamples> {
        if !self.before_first_resample {
            return None;
        }

        let input_buffer_start_pts = self.input_buffer_start_pts();

        // if entire input buffer is in the future
        if pts_range.1 < self.input_buffer_start_pts() {
            let duration = pts_range.1.saturating_sub(pts_range.0);
            let zero_samples = (duration.as_secs_f64() * self.output_sample_rate as f64) as usize;
            let samples = match self.channels {
                AudioChannels::Mono => AudioSamples::Mono(vec![0.0; zero_samples]),
                AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); zero_samples]),
            };
            return Some(samples);
        };

        if pts_range.0 < input_buffer_start_pts && input_buffer_start_pts < pts_range.1 {
            let duration = input_buffer_start_pts.saturating_sub(pts_range.0);
            let samples = (duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
            let batch = match self.channels {
                AudioChannels::Mono => AudioSamples::Mono(vec![0.0; samples]),
                AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); samples]),
            };
            trace!(samples, ?duration, "Add zero samples before first resample");
            self.resampler_input_buffer.push_front(batch)
        } else if pts_range.0 > input_buffer_start_pts {
            let duration = pts_range.0.saturating_sub(input_buffer_start_pts);
            let samples = (duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
            trace!(samples, ?duration, "Drain samples before first resample");
            self.resampler_input_buffer.drain_samples(samples);
        }

        None
    }

    fn resample(&mut self) {
        self.before_first_resample = false;
        let is_partial_read =
            self.resampler.input_frames_next() > self.resampler_input_buffer.frames();

        let indexing = match is_partial_read {
            true => {
                let partial_len = self.resampler_input_buffer.frames();
                trace!(partial_len, "Input buffer to small, partial resampling");
                Some(Indexing {
                    input_offset: 0,
                    output_offset: 0,
                    partial_len: Some(partial_len),
                    active_channels_mask: None,
                })
            }
            false => None,
        };
        let (consumed_samples, generated_samples) = match self.resampler.process_into_buffer(
            &self.resampler_input_buffer,
            &mut self.resampler_output_buffer,
            indexing.as_ref(),
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
        self.output_buffer
            .push_back(self.resampler_output_buffer.get_samples());

        // set that for the next iteration
        if is_partial_read {
            self.resampler_output_buffer.samples_to_drop = self.resampler.output_delay();
        }
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

    fn push_front(&mut self, batch: AudioSamples) {
        self.buffer.push_front((batch, 0));
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

#[derive(Debug)]
struct OutputBuffer {
    // oldest samples on the front, newest at the back
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

    fn frames(&self) -> usize {
        self.buffer
            .iter()
            .map(|(batch, bytes_read)| batch.len() - bytes_read)
            .sum()
    }

    fn push_back(&mut self, batch: AudioSamples) {
        self.buffer.push_back((batch, 0));
    }

    /// pad with zero if there is not enough
    fn read_chunk(&mut self, sample_count: usize) -> AudioSamples {
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

        // fill with zero if channel layouts would mismatch
        let range = 0..sample_count - samples.len();
        match &mut samples {
            AudioSamples::Mono(samples) => samples.extend(range.map(|_| 0.0)),
            AudioSamples::Stereo(samples) => samples.extend(range.map(|_| (0.0, 0.0))),
        };
        samples
    }
}
