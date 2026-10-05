use std::{collections::VecDeque, time::Duration};

use audioadapter::{Adapter, AdapterMut};
use rubato::{
    FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use tracing::{debug, error, trace, warn};

use crate::{
    AudioChannels, AudioSamples, Timestamp, prelude::InputAudioSamples, utils::AudioSamplesBuffer,
};

// Maximum *relative* deviation from the nominal resample ratio that we are willing to apply
// when stretching/squashing to correct drift. Rubato's `Async::new_sinc` is initialized with
// a static `max_resample_ratio_relative` of `1.0 + MAX_STRETCH_RATIO` (see
// `InputResampler::new`); going above that at runtime would be rejected by rubato.
//
// The 0.04 is the "useful" headroom (4%). The extra 0.001 is a small floating-point safety
// margin so that callers requesting exactly 4% don't trip the bound after clamping.
const MAX_STRETCH_RATIO: f64 = 0.04 + 0.001;

/// Per-input audio resampler with built-in drift correction.
///
/// ## Inputs (what arrives via `write_batch`)
/// `InputAudioSamples` batches from the queue. Each batch carries:
/// - `start_pts` — in the mixing clock (the queue already applied input offset/delay).
/// - `sample_rate` — fixed for the lifetime of this resampler; the calling `InputProcessor`
///   rebuilds us on a sample-rate or channel change.
/// - Mono or Stereo `f64` PCM samples.
///
/// Batches generally arrive in PTS order but may have gaps or overlaps; the queue does *not* pad
/// gaps.
///
/// ## Outputs (what `get_samples` produces)
/// Exactly the number of frames at `output_sample_rate` that fit the requested `pts_range`,
/// padded with silence if the input cannot keep up. We assume that caller will request
/// pts ranges that is multiple of whole samples.
///
/// ## Segments
/// Input is stored as a list of segments, each one a gap-free run of input. A new batch extends
/// the last segment, unless there is a gap of at least `SEAM_THRESHOLD` before it, then it starts
/// a new segment. Smaller gaps and overlaps are left to drift control. Writes never change state.
///
/// ## States
/// Output is produced in batches of `samples_in_batch` frames (one rubato run).
/// - `Running` — the front segment is played through rubato with drift control. Switches to
///   `ShouldSync` when the segment has less than 2 batches of input left (a gap follows or input
///   stopped arriving), or when drift is too large to correct. The last batch is faded out and
///   rubato is reset.
/// - `ShouldSync` — nothing is played, output is silent. Input older than the output position is
///   dropped. Switches to `Running` when the front segment starts within the requested range and
///   has at least 2 batches of input. Output is padded with zeros up to the segment start and the
///   first batch is faded in.
///
/// Requiring 2 batches of input guarantees that the fade-out always has a full batch.
///
/// After a reset, the leading frames of the first batch correspond to filter warmup; they're
/// discarded via `ResamplerOutputBuffer::samples_to_drop`.
///
/// ## Drift control
/// Input batches may arrive slightly early or late relative to the output timeline, so while
/// `Running` the resampler adjusts its rate to compensate.
///
/// Two timestamps drive the stretch/squash decision:
/// - `output_pts` — where the next output sample should land (in the mixing clock), computed
///   from `pts_range.0` plus what's already in `output_buffer`.
/// - `input_pts` — the mixing-clock PTS that the *next* output sample would actually have if we
///   ran rubato right now. Derived from the front segment start minus `original_output_delay`.
///
/// Their difference (the "drift") selects one of three branches:
/// - **stretch** — input is slightly behind: increase the resample ratio.
/// - **on-time** — drift within dead-band: ratio stays at 1.0.
/// - **squash** — input is slightly ahead: decrease the resample ratio.
///
/// Drift beyond `STRETCH_THRESHOLD` or `SQUASH_THRESHOLD` switches to `ShouldSync`.
pub(super) struct InputResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: AudioChannels,

    /// Input that hasn't been fed to rubato yet, one entry per gap-free run of input. Only the
    /// front segment is consumed.
    segments: VecDeque<InputSegment>,
    /// Fixed-size scratch buffer that rubato writes one batch of output frames into. Owns its
    /// own `samples_to_drop` counter for warmup discarding.
    resampler_output_buffer: ResamplerOutputBuffer,

    /// Holds resampled output frames between rubato runs. We drain from this to satisfy each
    /// `get_samples(pts_range)` call.
    output_buffer: AudioSamplesBuffer,

    resampler: rubato::Async<f64>,
    /// FIR filter delay of the resampler at construction time, as a Duration. Computed from
    /// `rubato.output_delay()` (a count of *output* frames) divided by `output_sample_rate`.
    /// Subtracted from `input_buffer_start_pts()` to get the PTS of the first warmup output
    /// sample in the input timeline.
    original_output_delay: Duration,

    state: ResamplerState,
}

enum ResamplerState {
    Running,
    ShouldSync,
}

struct InputSegment {
    /// PTS just past the last sample. Start PTS is derived from the frame count, so timestamp
    /// jitter inside a segment shows up as drift.
    end_pts: Timestamp,
    samples: AudioSamplesBuffer,
}

impl InputSegment {
    fn start_pts(&self, sample_rate: u32) -> Timestamp {
        self.end_pts - Duration::from_secs_f64(self.samples.frames() as f64 / sample_rate as f64)
    }
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

/// Drift dead-band. While `|input_start_pts - requested_start_pts| < 2ms` we leave the resample
/// ratio at 1.0 — too small to be worth correcting, and constantly toggling the ratio is itself
/// a source of artifacts.
const SHIFT_THRESHOLD: Duration = Duration::from_millis(2);

/// Maximum drift we'll *squash* (input is ahead of requested) before switching to the hard-drop
/// branch. Asymmetric with `STRETCH_THRESHOLD` because squashing only discards data — it doesn't
/// fabricate any — so a generous limit here mostly trades latency for smoothness.
const SQUASH_THRESHOLD: Duration = Duration::from_millis(500);

/// Maximum drift we'll *stretch* (input is behind requested) before switching to the gap-fill
/// branch. Smaller than `SQUASH_THRESHOLD` because stretching beyond a small fraction of a frame
/// is audibly bad.
const STRETCH_THRESHOLD: Duration = Duration::from_millis(40);

/// Minimal gap between consecutive batches that starts a new segment. Smaller gaps are treated as
/// timestamp jitter and left to drift control. Matches the shortest realistic packet loss (10ms
/// Opus packet).
const SEAM_THRESHOLD: Duration = Duration::from_millis(10);

impl InputResampler {
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        channels: AudioChannels,
    ) -> Result<Self, rubato::ResamplerConstructionError> {
        debug!(
            ?input_sample_rate,
            ?output_sample_rate,
            ?channels,
            "Create input resampler"
        );
        // Fixed *output* batch size for `FixedAsync::Output` mode: rubato will produce exactly
        // this many output frames per `process_into_buffer` call, consuming a variable number
        // of input frames to do so. At 48 kHz output, 256 frames ≈ 5.3 ms — small enough that
        // the stretch/squash decision in `get_samples` happens at fine granularity.
        let samples_in_batch = 256;

        let original_resampler_ratio = output_sample_rate as f64 / input_sample_rate as f64;
        let resampler = rubato::Async::<f64>::new_sinc(
            original_resampler_ratio,
            // Static upper bound on the *relative* ratio the resampler will accept at runtime.
            // Anything larger than this passed to `set_resample_ratio_relative` would be
            // rejected.
            1.0 + MAX_STRETCH_RATIO,
            Self::interpolation_params(input_sample_rate, output_sample_rate),
            samples_in_batch,
            match channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            },
            FixedAsync::Output,
        )?;
        // Number of *output* frames the rubato filter must "warm up" before it starts producing
        // meaningful samples. The first `output_delay` frames produced by the resampler are
        // essentially convolving the FIR window against zero-padded history; we drop them via
        // `resampler_output_buffer.samples_to_drop` below.
        let output_delay = resampler.output_delay();
        // rubato reports `output_delay` as a count of *output* frames, so we divide by
        // `output_sample_rate` to get the physical delay in seconds. (Dividing by
        // `input_sample_rate` would over-shift by a factor of `ratio` whenever the rates
        // differ.)
        let default_output_delay =
            Duration::from_secs_f64(output_delay as f64 / output_sample_rate as f64);

        let mut resampler_output_buffer = ResamplerOutputBuffer::new(channels, samples_in_batch);
        // Tell the output buffer to discard its first `output_delay` frames on the next read.
        // This effectively shifts the produced timeline so the *first emitted output sample*
        // corresponds to the *first input sample* (rather than to `-output_delay` worth of
        // zero-padded warmup).
        resampler_output_buffer.samples_to_drop = output_delay;

        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            channels,

            resampler,
            segments: VecDeque::new(),
            resampler_output_buffer,
            output_buffer: AudioSamplesBuffer::new(channels),

            original_output_delay: default_output_delay,

            state: ResamplerState::ShouldSync,
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

    /// Adjust rubato's resample ratio by a multiplicative factor relative to the nominal
    /// `output_sample_rate / input_sample_rate`. `rel_ratio == 1.0` means "no correction".
    fn set_resample_ratio_relative(&mut self, rel_ratio: f64) {
        let rel_ratio = rel_ratio.clamp(1.0 / (1.0 + MAX_STRETCH_RATIO), 1.0 + MAX_STRETCH_RATIO);
        if let Err(err) = self.resampler.set_resample_ratio_relative(rel_ratio, true) {
            warn!(%err, "Failed to update resampler ratio.");
            let _ = self.resampler.set_resample_ratio_relative(1.0, true);
        }
    }

    /// Append a newly arrived input batch to the last segment, or start a new segment if there
    /// is a gap of at least `SEAM_THRESHOLD`.
    pub fn write_batch(&mut self, batch: InputAudioSamples) {
        let (start_pts, end_pts) = batch.pts_range();
        trace!(
            ?start_pts,
            ?end_pts,
            len = batch.len(),
            "Resampler received a new batch"
        );

        if let Some(last) = self.segments.back_mut() {
            // If samples overlap too much drop, for lower overlap than 80ms we let squashing
            // handle that.
            if start_pts + Duration::from_millis(80) < last.end_pts {
                debug!("Detected overlapping batches, dropping.");
                return;
            }
            if start_pts < last.end_pts + SEAM_THRESHOLD {
                last.end_pts = end_pts;
                last.samples.push_back(batch.samples);
                return;
            }
            debug!(gap = ?(start_pts - last.end_pts), "Gap between batches, new segment.");
        }

        let mut samples = AudioSamplesBuffer::new(self.channels);
        samples.push_back(batch.samples);
        self.segments.push_back(InputSegment { end_pts, samples });
    }

    /// Produce exactly the number of output frames that fit `pts_range` at `output_sample_rate`.
    pub fn get_samples(&mut self, pts_range: (Timestamp, Timestamp)) -> AudioSamples {
        let batch_size = ((pts_range.1 - pts_range.0).as_secs_f64()
            * self.output_sample_rate as f64)
            .round() as usize;

        while self.output_buffer.frames() < batch_size {
            // Where the *next* output sample should land, accounting for what we've already
            // produced into `output_buffer`.
            let output_pts = pts_range.0
                + Duration::from_secs_f64(
                    self.output_buffer.frames() as f64 / self.output_sample_rate as f64,
                );

            match self.state {
                ResamplerState::Running => self.run(output_pts),
                ResamplerState::ShouldSync => {
                    if !self.sync(output_pts, pts_range.1) {
                        break; // `read_samples` pads the rest with zeros
                    }
                }
            }
        }
        self.output_buffer.read_samples(batch_size)
    }

    /// `Running` step: resample one batch with drift control, or switch to `ShouldSync`.
    fn run(&mut self, output_pts: Timestamp) {
        let Some(segment) = self.segments.front() else {
            error!("No input segment while running.");
            self.switch_to_sync();
            return;
        };
        let input_frames = segment.samples.frames();
        // PTS of the first sample that would be produced from resampler if current input was
        // resampled. It takes into account that something is already in the internal buffer.
        let input_pts = segment.start_pts(self.input_sample_rate) - self.original_output_delay;

        if input_frames < 2 * self.resampler.input_frames_next() {
            debug!(input_frames, "Input segment ending, switching to sync.");
            self.switch_to_sync();
            return;
        }
        if input_pts > output_pts + STRETCH_THRESHOLD || input_pts + SQUASH_THRESHOLD < output_pts {
            debug!(drift = ?(input_pts - output_pts), "Drift too large, switching to sync.");
            self.switch_to_sync();
            return;
        }

        if input_pts > output_pts + SHIFT_THRESHOLD {
            // === STRETCH ===
            let drift = input_pts - output_pts;
            let drift_ratio = drift.as_secs_f64() / STRETCH_THRESHOLD.as_secs_f64();
            // multiply by 2.0 so max resampling is reached at the half point
            // of the stretch limit
            let ratio = 2.0 * MAX_STRETCH_RATIO * drift_ratio;

            self.set_resample_ratio_relative(1.0 + ratio);
            trace!(ratio, ?drift, "Input buffer behind, stretching");
        } else if input_pts + SHIFT_THRESHOLD > output_pts {
            // === ON-TIME (dead-band) ===
            self.set_resample_ratio_relative(1.0);
            trace!("Input buffer on time");
        } else {
            // === SQUASH ===
            let drift = output_pts - input_pts;
            let drift_ratio = drift.as_secs_f64() / SQUASH_THRESHOLD.as_secs_f64();
            // multiply by 2.0 so max resampling is reached at the half point
            // of the squash limit
            let ratio = 2.0 * MAX_STRETCH_RATIO * drift_ratio;

            self.set_resample_ratio_relative(1.0 - ratio);
            trace!(ratio, ?drift, "Input buffer ahead, squashing");
        }

        let batch = self.resample();
        self.output_buffer.push_back(batch);
    }

    /// `ShouldSync` step: drop input older than `output_pts` and switch to `Running` if the front
    /// segment can start before `end_pts`. Returns false if there is nothing to play yet.
    fn sync(&mut self, output_pts: Timestamp, end_pts: Timestamp) -> bool {
        while let Some(segment) = self.segments.front_mut() {
            if segment.end_pts <= output_pts {
                trace!(end_pts = ?segment.end_pts, "Drop input segment from the past");
                self.segments.pop_front();
                continue;
            }
            let start_pts = segment.start_pts(self.input_sample_rate);
            if start_pts < output_pts {
                let duration = output_pts - start_pts;
                let samples = (duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
                trace!(samples, ?duration, "Drop input samples from the past");
                segment.samples.drain_samples(samples);
            }
            break;
        }

        let Some(segment) = self.segments.front() else {
            return false;
        };
        let start_pts = segment.start_pts(self.input_sample_rate);
        if start_pts >= end_pts || segment.samples.frames() < 2 * self.resampler.input_frames_next()
        {
            return false;
        }

        // Pad output with zeros, so the first sample of the segment lands at its PTS.
        if start_pts > output_pts {
            let duration = start_pts - output_pts;
            let samples =
                (duration.as_secs_f64() * self.output_sample_rate as f64).round() as usize;
            trace!(
                samples,
                ?duration,
                "Pad output up to the input segment start"
            );
            self.output_buffer.push_back(match self.channels {
                AudioChannels::Mono => AudioSamples::Mono(vec![0.0; samples]),
                AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); samples]),
            });
        }
        self.switch_to_running();
        true
    }

    /// Rubato was reset, so the first batch is faded in.
    fn switch_to_running(&mut self) {
        let mut batch = self.resample();
        fade(&mut batch, Fade::In);
        self.output_buffer.push_back(batch);
        self.state = ResamplerState::Running;
    }

    /// The last batch is faded out, so dropping rubato's internal state is silent.
    fn switch_to_sync(&mut self) {
        let mut batch = self.resample();
        fade(&mut batch, Fade::Out);
        self.output_buffer.push_back(batch);
        self.resampler.reset();
        self.resampler_output_buffer.samples_to_drop = self.resampler.output_delay();
        self.state = ResamplerState::ShouldSync;
    }

    /// Run rubato once on the front segment and return one batch of output frames.
    fn resample(&mut self) -> AudioSamples {
        let Some(segment) = self.segments.front_mut() else {
            error!("No input segment to resample.");
            return match self.channels {
                AudioChannels::Mono => AudioSamples::Mono(Vec::new()),
                AudioChannels::Stereo => AudioSamples::Stereo(Vec::new()),
            };
        };
        let missing_input_samples = self
            .resampler
            .input_frames_next()
            .saturating_sub(segment.samples.frames());

        // Should not happen, states switch before input runs out.
        let indexing = match missing_input_samples > 0 {
            true => {
                let partial_len = segment.samples.frames();
                warn!(partial_len, "Input buffer to small, partial resampling");
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
            &segment.samples,
            &mut self.resampler_output_buffer,
            indexing.as_ref(),
        ) {
            Ok(result) => result,
            Err(err) => {
                // Hard failure path: emit silence rather than stalling the mixer. We pretend
                // the full output buffer was generated so the caller can keep advancing.
                error!("Resampling error: {err}");
                self.resampler_output_buffer.fill_with(&0.0);
                (0, self.resampler_output_buffer.frames())
            }
        };

        segment.samples.drain_samples(consumed_samples);
        if generated_samples != self.resampler_output_buffer.frames() {
            error!(
                expected = self.resampler_output_buffer.frames(),
                actual = generated_samples,
                "Resampler generated wrong amount of samples"
            )
        }
        self.resampler_output_buffer.get_samples()
    }
}

enum Fade {
    In,
    Out,
}

/// Multiply samples by a linear ramp, 0 to 1 for `Fade::In`, 1 to 0 for `Fade::Out`.
fn fade(samples: &mut AudioSamples, fade: Fade) {
    let len = samples.len() as f64;
    let gain = |i: usize| match fade {
        Fade::In => i as f64 / len,
        Fade::Out => 1.0 - (i + 1) as f64 / len,
    };
    match samples {
        AudioSamples::Mono(samples) => {
            for (i, sample) in samples.iter_mut().enumerate() {
                *sample *= gain(i);
            }
        }
        AudioSamples::Stereo(samples) => {
            for (i, (l, r)) in samples.iter_mut().enumerate() {
                *l *= gain(i);
                *r *= gain(i);
            }
        }
    }
}

/// Fixed-size scratch buffer that rubato writes into.
///
/// The buffer's length is `samples_in_batch` (set at construction); each `resample()` call
/// overwrites its contents in full. The `audioadapter::AdapterMut` impl below is what rubato
/// calls into.
///
/// `samples_to_drop` is non-zero whenever the *next* read should skip a leading prefix — set on
/// construction (initial filter warmup) and again after every rubato reset (switch to
/// `ShouldSync`).
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

    /// Take a copy of the current buffer contents, skipping the first `samples_to_drop` frames
    /// if non-zero. Resets `samples_to_drop` to 0 after a single read — repeat reads of the
    /// same buffer would not have the same skip applied.
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

#[cfg(test)]
mod equal_sample_rate_tests;
#[cfg(test)]
mod test_utils;
