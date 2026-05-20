use std::time::Duration;

use audioadapter::{Adapter, AdapterMut};
use rubato::{
    FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use tracing::{debug, error, trace, warn};

use crate::{AudioChannels, AudioSamples, prelude::InputAudioSamples, utils::AudioSamplesBuffer};

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
/// Batches generally arrive in PTS order but may have small gaps or overlaps; the queue does
/// *not* pad gaps.
///
/// ## Outputs (what `get_samples` produces)
/// Exactly the number of frames at `output_sample_rate` that fit the requested `pts_range`,
/// padded with silence if the input cannot keep up. We assume that caller will request
/// pts ranges that is multiple of whole samples.
///
/// ## Data flow
/// 1. Incoming batches are appended to `resampler_input_buffer` (with overlap drop).
/// 2. `get_samples` runs `resample()` in a loop, each call moves a fixed `samples_in_batch`
///    worth of *output* frames from the rubato resampler into `output_buffer`, until
///    `output_buffer` has enough to satisfy the requested range.
/// 3. The leading frames of the very first resample (or first after discontinuity) correspond
///    to filter warmup (samples the resampler hasn't fully "seen" yet); they're discarded via
///    `ResamplerOutputBuffer::samples_to_drop`.
///
/// ## Drift control
/// Input batches may arrive slightly early or late relative to the output timeline, so the
/// resampler adjusts its rate to compensate.
///
/// Two timestamps drive the stretch/squash decision in `get_samples`:
/// - `requested_start_pts` — where the next output sample should land (in the mixing clock),
///   computed from `pts_range.0` plus what's already in `output_buffer`.
/// - `input_start_pts` — the mixing-clock PTS that the *next* output sample would actually
///   have if we ran rubato right now. Derived from `input_buffer_start_pts()` minus
///   `original_output_delay`.
///
/// Their difference (the "drift") selects one of five branches:
/// - **gap-fill** — input is far behind: prepend zeros to the input buffer.
/// - **stretch** — input is slightly behind: increase the resample ratio.
/// - **on-time** — drift within dead-band: ratio stays at 1.0.
/// - **squash** — input is slightly ahead: decrease the resample ratio.
/// - **drop** — input is far ahead: discard excess input samples.
///
/// Note: because we make the decision per-resample-iteration (not per-batch), we can decide
/// to squash even if `resampler_input_buffer` doesn't yet contain a full batch — rubato's
/// partial-resample path handles that.
pub(super) struct InputResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: AudioChannels,

    /// Pending input PCM that hasn't been fed to rubato yet. Frames are consumed (drained) from
    /// the front each time `resample()` runs. May also have zeros pushed to the front (gap-fill
    /// before first resample, or in the `get_samples` gap branch) or samples drained from the
    /// front (drop branch).
    resampler_input_buffer: AudioSamplesBuffer,
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
    /// Nominal ratio = `output_sample_rate / input_sample_rate`. We multiply it by a "relative"
    /// factor in [1/(1+MAX), 1+MAX] when correcting drift.
    original_resampler_ratio: f64,

    /// PTS just past the last sample currently held in `resampler_input_buffer`. Updated only in
    /// `write_batch`. Combined with the buffer's frame count, it lets us compute
    /// `input_buffer_start_pts()` on demand.
    input_buffer_end_pts: Duration,

    /// Synchronization gate. While true, `get_samples` either serves any frames already in
    /// `output_buffer` (padded with zeros) when the input is entirely in the future, or aligns
    /// the input buffer to the requested range (via `maybe_prepare_before_resample`) and *does
    /// not* engage the stretch/squash logic. Cleared by `resample()`; re-armed by
    /// `reset_after_discontinuity` so the next `get_samples` re-runs the gate against fresh
    /// input.
    needs_input_resync: bool,
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
            resampler_input_buffer: AudioSamplesBuffer::new(channels),
            resampler_output_buffer,
            output_buffer: AudioSamplesBuffer::new(channels),

            original_output_delay: default_output_delay,
            original_resampler_ratio,
            input_buffer_end_pts: Duration::ZERO,

            needs_input_resync: true,
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

    /// Adjust rubato's resample ratio by a multiplicative factor relative to
    /// `original_resampler_ratio`. `rel_ratio == 1.0` means "no correction".
    fn set_resample_ratio_relative(&mut self, rel_ratio: f64) {
        let rel_ratio = rel_ratio.clamp(1.0 / (1.0 + MAX_STRETCH_RATIO), 1.0 + MAX_STRETCH_RATIO);
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

    /// Append a newly arrived input batch to `resampler_input_buffer`.
    pub fn write_batch(&mut self, batch: InputAudioSamples) {
        let (start_pts, end_pts) = batch.pts_range();
        trace!(
            ?start_pts,
            ?end_pts,
            len = batch.len(),
            "Resampler received a new batch"
        );

        // If samples overlap to much drop, for lower overlap than 80ms we let
        // squashing handle that
        if start_pts + Duration::from_millis(80) < self.input_buffer_end_pts {
            debug!("Detected overlapping batches, dropping.");
            return;
        }

        // This defines `input_buffer_end_pts()` results
        self.input_buffer_end_pts = end_pts;
        self.resampler_input_buffer.push_back(batch.samples);
    }

    /// Produce exactly the number of output frames that fit `pts_range` at `output_sample_rate`.
    /// The decision-loop body runs once per `samples_in_batch` worth of output frames produced
    /// (because rubato emits a fixed-output-size batch per call).
    pub fn get_samples(&mut self, pts_range: (Duration, Duration)) -> AudioSamples {
        // Initial synchronization on init or after reset
        if let Some(batch) = self.maybe_prepare_before_resample(pts_range) {
            return batch; // zeros or flush `self.output_buffer`
        };

        let batch_size = ((pts_range.1 - pts_range.0).as_secs_f64()
            * self.output_sample_rate as f64)
            .round() as usize;

        while self.output_buffer.frames() < batch_size {
            // Where the *next* output sample we still owe should land, accounting for what
            // we've already produced into `output_buffer`.
            let requested_start_pts = pts_range.0
                + Duration::from_secs_f64(
                    self.output_buffer.frames() as f64 / self.output_sample_rate as f64,
                );

            // PTS of the first timestamp that would be produced from resampler if current input
            // buffer was resampled. It takes into account that something is already in the
            // internal buffer.
            let input_start_pts = self
                .input_buffer_start_pts()
                .saturating_sub(self.original_output_delay);

            if input_start_pts > requested_start_pts + STRETCH_THRESHOLD {
                // === GAP-FILL ===
                // `self.input_buffer_start_pts()` is too much in the future. Too much to try
                // to stretch, so prepend zeros to input buffer.

                // NOTE: In current implementation this should never happen, because queue
                // does not allow sending too much ahead. This case will be relevant if we
                // move resampler to the queue.
                let gap = input_start_pts.saturating_sub(requested_start_pts);
                let sample_count = (gap.as_secs_f64() * self.input_sample_rate as f64) as usize;
                let samples = match self.channels {
                    AudioChannels::Mono => AudioSamples::Mono(vec![0.0; sample_count]),
                    AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); sample_count]),
                };
                self.resampler_input_buffer.push_front(samples);
                self.set_resample_ratio_relative(1.0);
                debug!(
                    sample_count,
                    ?gap,
                    "Input buffer behind, writing zeroes samples"
                )
            } else if input_start_pts > requested_start_pts + SHIFT_THRESHOLD {
                // === STRETCH ===
                let drift = input_start_pts.saturating_sub(requested_start_pts);
                let drift_ratio = drift.as_secs_f64() / STRETCH_THRESHOLD.as_secs_f64();
                // multiply by 2.0 so max resampling is reached at the half point
                // of the stretch limit
                let ratio = 2.0 * MAX_STRETCH_RATIO * drift_ratio;

                self.set_resample_ratio_relative(1.0 + ratio);
                trace!(ratio, ?drift, "Input buffer behind, stretching");
            } else if input_start_pts + SHIFT_THRESHOLD > requested_start_pts {
                // === ON-TIME (dead-band) ===
                // |drift| < SHIFT_THRESHOLD; leave the ratio alone.
                self.set_resample_ratio_relative(1.0);
                trace!("Input buffer on time");
            } else if input_start_pts + SQUASH_THRESHOLD > requested_start_pts {
                // === SQUASH ===
                let drift = requested_start_pts.saturating_sub(input_start_pts);
                let drift_ratio = drift.as_secs_f64() / SQUASH_THRESHOLD.as_secs_f64();
                // multiply by 2.0 so max resampling is reached at the half point
                // of the squash limit
                let ratio = 2.0 * MAX_STRETCH_RATIO * drift_ratio;

                self.set_resample_ratio_relative(1.0 - ratio);
                trace!(ratio, ?drift, "Input buffer ahead, squashing");
            } else {
                // === DROP ===
                // `self.input_buffer_start_pts()` is too much "behind" to recover by squashing.

                // TODO: handle discontinuity (same caveat as gap-fill — the filter state is
                // now stale relative to the post-drop signal).
                let duration_to_drop = requested_start_pts.saturating_sub(input_start_pts);
                let samples_to_drop =
                    (duration_to_drop.as_secs_f64() * self.input_sample_rate as f64) as usize;
                self.resampler_input_buffer.drain_samples(samples_to_drop);
                self.set_resample_ratio_relative(1.0);
                debug!(
                    samples_to_drop,
                    ?duration_to_drop,
                    "Input buffer ahead, dropping samples"
                );
            }

            // One rubato batch's worth of output frames lands in `output_buffer`. Loop continues
            // until we have enough — unless the run was partial (input ran out and rubato was
            // fed zero-padding), in which case we reset the FIR state on this side of the
            // discontinuity and let `read_samples` pad the shortfall with zeros. The next
            // `get_samples` re-aligns via the gate.
            if let ResampleResult::Partial = self.resample() {
                self.reset_after_discontinuity();
                break;
            }
        }
        self.output_buffer.read_samples(batch_size)
    }

    /// Pre-resample synchronization gate. Active while `needs_input_resync` is set
    /// (initially, and after `reset_after_discontinuity`). Aligns `resampler_input_buffer`
    /// so its earliest sample's PTS equals `pts_range.0`:
    fn maybe_prepare_before_resample(
        &mut self,
        pts_range: (Duration, Duration),
    ) -> Option<AudioSamples> {
        if !self.needs_input_resync {
            return None;
        }

        let input_buffer_start_pts = self.input_buffer_start_pts();

        // If entire input buffer is in the future or input buffer is empty
        // Then flush output buffer (or return zeros)
        if self.resampler_input_buffer.frames() == 0 || pts_range.1 < input_buffer_start_pts {
            let batch_size = ((pts_range.1 - pts_range.0).as_secs_f64()
                * self.output_sample_rate as f64)
                .round() as usize;

            // on first run it will just return zeros, but after discontinuity
            // it might flush rest of previous run
            return Some(self.output_buffer.read_samples(batch_size));
        }

        // If input buffer starts in the middle of requested ranges
        // Then pad with zeros at the front of input buffer
        if pts_range.0 < input_buffer_start_pts && input_buffer_start_pts < pts_range.1 {
            let duration = input_buffer_start_pts.saturating_sub(pts_range.0);
            let samples = (duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
            let batch = match self.channels {
                AudioChannels::Mono => AudioSamples::Mono(vec![0.0; samples]),
                AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); samples]),
            };
            trace!(
                samples,
                ?duration,
                "Add zero samples at the initial resample"
            );
            self.resampler_input_buffer.push_front(batch);
            return None;
        }

        // If input buffer start before requested range
        // Then drop samples that are too older
        if pts_range.0 > input_buffer_start_pts {
            // Drop too-old samples so the buffer starts at `pts_range.0`.
            let duration = pts_range.0.saturating_sub(input_buffer_start_pts);
            let samples = (duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
            trace!(samples, ?duration, "Drain samples before first resample");
            self.resampler_input_buffer.drain_samples(samples);
            return None;
        }

        None
    }

    /// Run rubato once: feed input from `resampler_input_buffer`, push a batch of output frames
    /// onto `output_buffer`, and clear `needs_input_resync`.
    fn resample(&mut self) -> ResampleResult {
        self.needs_input_resync = false;
        let missing_input_samples = self
            .resampler
            .input_frames_next()
            .saturating_sub(self.resampler_input_buffer.frames());

        let indexing = match missing_input_samples > 0 {
            true => {
                let partial_len = self.resampler_input_buffer.frames();
                debug!(partial_len, "Input buffer to small, partial resampling");
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
                // Hard failure path: emit silence rather than stalling the mixer. We pretend
                // the full output buffer was generated so the caller can keep advancing.
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

        if missing_input_samples > 0 {
            ResampleResult::Partial
        } else {
            ResampleResult::Full
        }
    }

    /// Reset state that becomes invalid across an input discontinuity
    /// - `resampler` — clears rubato's internal sample buffer and FIR history so the next batch
    ///   isn't convolved against pre-gap audio. `reset()` also restores rubato's ratio to its
    ///   original value;
    /// - `resampler_output_buffer.samples_to_drop` — re-arm the warmup discard so the next read
    ///   skips the freshly re-introduced `output_delay` worth of meaningless filter prefix.
    /// - `needs_input_resync` — re-engage `maybe_prepare_before_resample` so the next
    ///   `get_samples` call realigns the (now empty) input buffer against the requested PTS
    ///   range before resampling.
    fn reset_after_discontinuity(&mut self) {
        self.resampler.reset();
        self.resampler_output_buffer.samples_to_drop = self.resampler.output_delay();
        self.needs_input_resync = true;
    }
}

enum ResampleResult {
    Full,
    Partial,
}

/// Fixed-size scratch buffer that rubato writes into.
///
/// The buffer's length is `samples_in_batch` (set at construction); each `resample()` call
/// overwrites its contents in full. The `audioadapter::AdapterMut` impl below is what rubato
/// calls into.
///
/// `samples_to_drop` is non-zero whenever the *next* read should skip a leading prefix — set on
/// construction (initial filter warmup) and again after partial-resample runs (effective
/// re-warming).
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
