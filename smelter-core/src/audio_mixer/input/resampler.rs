use std::time::Duration;

use audioadapter::{Adapter, AdapterMut};
use rubato::{
    FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use tracing::{debug, error, trace, warn};

use crate::{AudioChannels, AudioSamples, prelude::InputAudioSamples, utils::AudioSamplesBuffer};

// Maximum *relative* deviation from the nominal resample ratio that we are
// willing to apply when stretching/squashing to correct drift. Rubato's
// `Async::new_sinc` is initialized with a static `max_resample_ratio_relative`
// of `1.0 + MAX_STRETCH_RATIO` (see `InputResampler::new`); going above that
// at runtime would be rejected by rubato.
//
// The 0.04 is the "useful" headroom (4%). The extra 0.001 is a small
// floating-point safety margin so that callers requesting exactly 4% don't
// trip the bound after clamping.
const MAX_STRETCH_RATIO: f64 = 0.04 + 0.001;

/// Per-input audio resampler with built-in drift correction.
///
/// ## Inputs (what arrives via `write_batch`)
/// `InputAudioSamples` batches from the queue, each with a `start_pts` (in the
/// mixing clock — the queue already applied input offset/delay), a fixed
/// `sample_rate` (constant for the lifetime of this resampler — the calling
/// `InputProcessor` rebuilds us on a sample-rate or channel change), and
/// either Mono or Stereo `f64` PCM samples. Batches funcerally arrive in PTS
/// order but may have small gaps or overlaps; the queue does *not* pad gaps.
///
/// ## Outputs (what `get_samples` produces)
/// Exactly the number of frames at `output_sample_rate` that fit the
/// requested `pts_range`, padded with silence if the input cannot keep up.
///
/// ## Data flow
/// 1. Incoming batches are appended to `resampler_input_buffer` (with gap zero-fill / overlap drop).
/// 2. `get_samples` runs `resample()` in a loop, each call moves a fixed
///    `samples_in_batch` worth of *output* frames from the rubato resampler
///    into `output_buffer`, until `output_buffer` has enough to satisfy the
///    requested range.
/// 3. The leading frames of the very first resample correspond to filter
///    warmup (samples the resampler hasn't fully "seen" yet); they're
///    discarded via `ResamplerOutputBuffer::samples_to_drop`.
///
/// ## Drift control
/// Two timestamps drive the stretch/squash decision in `get_samples`:
/// - `requested_start_pts` — where the next output sample should land (in the
///   mixing clock), computed from `pts_range.0` plus what's already in
///   `output_buffer`.
/// - `input_start_pts` — the mixing-clock PTS that the *next* output sample
///   would actually have if we ran rubato right now. Derived from
///   `input_buffer_start_pts()` minus `original_output_delay` (the FIR
///   filter delay expressed in input-sample time — see the field doc on
///   `original_output_delay` for why the unit matters).
///
/// Their difference (the "drift") selects one of five branches:
/// gap-fill / stretch / on-time / squash / drop (see `get_samples`).
///
/// Note: because we make the decision per-resample-iteration (not per-batch),
/// we can decide to squash even if `resampler_input_buffer` doesn't yet
/// contain a full batch — rubato's partial-resample path handles that.
pub(super) struct InputResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: AudioChannels,

    /// Pending input PCM that hasn't been fed to rubato yet. Frames are
    /// consumed (drained) from the front each time `resample()` runs. May
    /// also have zeros pushed to the front (gap-fill before first resample,
    /// or in the `get_samples` gap branch) or samples drained from the front
    /// (drop branch).
    resampler_input_buffer: AudioSamplesBuffer,
    /// Fixed-size scratch buffer that rubato writes one batch of output frames
    /// into. Owns its own `samples_to_drop` counter for warmup discarding.
    resampler_output_buffer: ResamplerOutputBuffer,

    /// Holds resampled output frames between rubato runs. We drain from this
    /// to satisfy each `get_samples(pts_range)` call.
    output_buffer: AudioSamplesBuffer,

    resampler: rubato::Async<f64>,
    /// FIR filter delay of the resampler at construction time, as a
    /// Duration. Computed from `rubato.output_delay()` (a count of
    /// *output* frames) divided by `output_sample_rate`. Subtracted from
    /// `input_buffer_start_pts()` to get the PTS of the first warmup
    /// output sample in the input timeline.
    original_output_delay: Duration,
    /// Nominal ratio = `output_sample_rate / input_sample_rate`. We multiply
    /// it by a "relative" factor in [1/(1+MAX), 1+MAX] when correcting drift.
    original_resampler_ratio: f64,

    /// PTS just past the last sample currently held in `resampler_input_buffer`.
    /// Updated only in `write_batch`. Combined with the buffer's frame count,
    /// it lets us compute `input_buffer_start_pts()` on demand.
    input_buffer_end_pts: Duration,

    /// Synchronization gate. While true, `get_samples` either emits silence
    /// (input is entirely in the future) or aligns the input buffer to the
    /// requested range (via `maybe_prepare_before_resample`) and *does not*
    /// engage the stretch/squash logic. Cleared on the first call to
    /// `resample()`.
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

/// In `write_batch`, if a new batch's `start_pts` is more than this far past
/// `input_buffer_end_pts`, we treat it as a gap and zero-fill (instead of
/// trusting the timestamp jitter). Symmetrically, if the new batch starts
/// more than this far *before* `input_buffer_end_pts`, we drop it as an
/// overlap.
///
/// 80ms also matches the queue's `MIXER_STRETCH_BUFFER` look-ahead, which
/// may or may not be intentional.
const CONTINUITY_THRESHOLD: Duration = Duration::from_millis(80);

/// Drift dead-band. While `|input_start_pts - requested_start_pts| < 2ms`
/// we leave the resample ratio at 1.0 — too small to be worth correcting,
/// and constantly toggling the ratio is itself a source of artifacts.
const SHIFT_THRESHOLD: Duration = Duration::from_millis(2);

/// Maximum drift we'll *squash* (input is ahead of requested) before
/// switching to the hard-drop branch. Asymmetric with `STRETCH_THRESHOLD`
/// because squashing only discards data — it doesn't fabricate any — so a
/// funcerous limit here mostly trades latency for smoothness.
const SQUASH_THRESHOLD: Duration = Duration::from_millis(500);

/// Maximum drift we'll *stretch* (input is behind requested) before
/// switching to the gap-fill branch. Smaller than `SQUASH_THRESHOLD` because
/// stretching beyond a small fraction of a frame is audibly bad.
const STRETCH_THRESHOLD: Duration = Duration::from_millis(40);

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
        // Fixed *output* batch size for `FixedAsync::Output` mode: rubato will
        // produce exactly this many output frames per `process_into_buffer`
        // call, consuming a variable number of input frames to do so. At
        // 48 kHz output, 256 frames ≈ 5.3 ms — small enough that the
        // stretch/squash decision in `get_samples` happens at fine granularity.
        let samples_in_batch = 256;

        let original_resampler_ratio = output_sample_rate as f64 / input_sample_rate as f64;
        let resampler = rubato::Async::<f64>::new_sinc(
            original_resampler_ratio,
            // Static upper bound on the *relative* ratio the resampler will
            // accept at runtime. Anything larger than this passed to
            // `set_resample_ratio_relative` would be rejected.
            1.0 + MAX_STRETCH_RATIO,
            Self::interpolation_params(input_sample_rate, output_sample_rate),
            samples_in_batch,
            match channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            },
            FixedAsync::Output,
        )?;
        // Number of *output* frames the rubato filter must "warm up" before it
        // starts producing meaningful samples. The first `output_delay` frames
        // produced by the resampler are essentially convolving the FIR window
        // against zero-padded history; we drop them via
        // `resampler_output_buffer.samples_to_drop` below.
        let output_delay = resampler.output_delay();
        // rubato reports `output_delay` as a count of *output* frames, so
        // we divide by `output_sample_rate` to get the physical delay in
        // seconds. (Dividing by `input_sample_rate` would over-shift by a
        // factor of `ratio` whenever the rates differ.)
        let default_output_delay =
            Duration::from_secs_f64(output_delay as f64 / output_sample_rate as f64);

        let mut resampler_output_buffer = ResamplerOutputBuffer::new(channels, samples_in_batch);
        // Tell the output buffer to discard its first `output_delay` frames
        // on the next read. This effectively shifts the produced timeline so
        // the *first emitted output sample* corresponds to the *first input
        // sample* (rather than to `-output_delay` worth of zero-padded warmup).
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

    /// PTS of the oldest (front) sample currently in `resampler_input_buffer`,
    /// derived from `input_buffer_end_pts` minus the duration of buffered
    /// frames. This stays consistent across `drain_samples`, push_front of
    /// zeros, and resample-time consumption because all of those change
    /// `frames()` without touching `input_buffer_end_pts`.
    fn input_buffer_start_pts(&self) -> Duration {
        self.input_buffer_end_pts
            .saturating_sub(Duration::from_secs_f64(
                self.resampler_input_buffer.frames() as f64 / self.input_sample_rate as f64,
            ))
    }

    /// Adjust rubato's resample ratio by a multiplicative factor relative to
    /// `original_resampler_ratio`. `rel_ratio == 1.0` means "no correction".
    ///
    /// We:
    /// - Clamp to the static range rubato was constructed with
    ///   (`±MAX_STRETCH_RATIO`).
    /// - Skip the call when the absolute change is tiny (<0.01) — except we
    ///   *always* honour the transition out of an exact 1.0, otherwise we'd
    ///   refuse to start correcting tiny drifts.
    /// - On rubato error, fall back to ratio 1.0 to avoid leaving the filter
    ///   stuck in an undesired state.
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
    ///
    /// Three cases:
    /// - **Gap > CONTINUITY_THRESHOLD ahead of buffer end**: zero-fill the
    ///   gap so the timeline stays contiguous. We skip this when the buffer
    ///   is empty — in that case `input_buffer_start_pts` automatically
    ///   resolves to the new batch's `start_pts`, so there is no anchor to
    ///   preserve and no reason to invent silence.
    /// - **Batch starts > CONTINUITY_THRESHOLD before buffer end**: treat as
    ///   an overlap and drop the entire batch. We don't try to splice
    ///   partial overlap; the threshold is small enough that lost audio is
    ///   negligible in practice.
    /// - **Otherwise** (small jitter either way): trust the new batch's
    ///   `end_pts` as the new authoritative `input_buffer_end_pts`. Small
    ///   timestamp drift will surface later as `get_samples` drift.
    pub fn write_batch(&mut self, batch: InputAudioSamples) {
        let (start_pts, end_pts) = batch.pts_range();
        trace!(
            ?start_pts,
            ?end_pts,
            len = batch.len(),
            "Resampler received a new batch"
        );

        // Only zero-fill when there is still pending data in the buffer that needs
        // an accurate time anchor relative to the new batch. With an empty buffer,
        // `input_buffer_start_pts` resolves to `batch.start_pts` automatically.
        if start_pts > self.input_buffer_end_pts + CONTINUITY_THRESHOLD
            && self.resampler_input_buffer.frames() > 0
        {
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
        // Note: this overwrites `input_buffer_end_pts` even on small forward
        // gaps (<= CONTINUITY_THRESHOLD). The actual buffered frame count is
        // not adjusted, so `input_buffer_start_pts()` will then "appear" to
        // shift forward — which is fine: that *is* the new ground truth.
        self.input_buffer_end_pts = end_pts;

        self.resampler_input_buffer.push_back(batch.samples);
    }

    /// Produce exactly the number of output frames that fit `pts_range` at
    /// `output_sample_rate`. The decision-loop body runs once per
    /// `samples_in_batch` worth of output frames produced (because rubato
    /// emits a fixed-output-size batch per call).
    pub fn get_samples(&mut self, pts_range: (Duration, Duration)) -> AudioSamples {
        // Pre-resample synchronization gate. If we're still in the warmup
        // phase, this either returns an all-zeros batch immediately or
        // pads/drains the input buffer so the very first resample lines up.
        if let Some(zero_batch) = self.maybe_prepare_before_resample(pts_range) {
            return zero_batch;
        };

        let batch_size = ((pts_range.1 - pts_range.0).as_secs_f64()
            * self.output_sample_rate as f64)
            .round() as usize;

        while self.output_buffer.frames() < batch_size {
            // Where the *next* output sample we still owe should land,
            // accounting for what we've already produced into `output_buffer`.
            let requested_start_pts = pts_range.0
                + Duration::from_secs_f64(
                    self.output_buffer.frames() as f64 / self.output_sample_rate as f64,
                );

            // PTS of the first timestamp that would be produced from
            // resampler if current input buffer was resampled. It takes
            // into account that something is already in the internal
            // buffer.
            //
            // The `- original_output_delay` shift moves
            // `input_buffer_start_pts()` back by the FIR filter delay,
            // so the resulting PTS matches the *first* output sample the
            // resampler will produce — including the leading warmup
            // samples that `ResamplerOutputBuffer::samples_to_drop`
            // discards on the next read. After that discard the first
            // *kept* output sample's PTS equals
            // `input_buffer_start_pts()` again. The shift remains in
            // effect after warmup, on the assumption filter latency is
            // approximately constant.
            let input_start_pts = self
                .input_buffer_start_pts()
                .saturating_sub(self.original_output_delay);

            if input_start_pts > requested_start_pts + STRETCH_THRESHOLD {
                // === GAP-FILL ===
                // Input is too far in the future to bridge by stretching.
                // Prepend zeros to the input buffer so the next resample
                // run produces silence aligned with the requested PTS.
                // TODO: handle discontinuity (the resampler's filter state
                // still carries energy from the prior signal).
                let gap_duration = input_start_pts.saturating_sub(requested_start_pts);
                let zero_samples =
                    (gap_duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
                let samples = match self.channels {
                    AudioChannels::Mono => AudioSamples::Mono(vec![0.0; zero_samples]),
                    AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); zero_samples]),
                };
                self.resampler_input_buffer.push_front(samples);
                self.set_resample_ratio_relative(1.0);
                debug!(
                    zero_samples,
                    ?gap_duration,
                    "Input buffer behind, writing zeroes samples"
                )
            } else if input_start_pts > requested_start_pts + SHIFT_THRESHOLD {
                // === STRETCH ===
                // Input is slightly ahead (we're producing output that should
                // be at a PTS earlier than what the input can yet supply).
                // Slow rubato down so it consumes fewer input frames per
                // output frame. The correction ramps linearly with drift,
                // reaching MAX at half of STRETCH_THRESHOLD — functler at
                // first, then more aggressive as we approach the gap branch.
                let drift = input_start_pts.saturating_sub(requested_start_pts);
                let ratio = drift.as_secs_f64() / STRETCH_THRESHOLD.as_secs_f64();

                // multiply by 2.0 so max resampling is reached at the half point
                // of the stretch limit
                self.set_resample_ratio_relative(1.0 + (2.0 * MAX_STRETCH_RATIO * ratio));
                trace!(ratio, ?drift, "Input buffer behind, stretching");
            } else if input_start_pts + SHIFT_THRESHOLD > requested_start_pts {
                // === ON-TIME (dead-band) ===
                // |drift| < SHIFT_THRESHOLD; leave the ratio alone.
                self.set_resample_ratio_relative(1.0);
                trace!("Input buffer on time");
            } else if input_start_pts + SQUASH_THRESHOLD > requested_start_pts {
                // === SQUASH ===
                // Mirror of STRETCH: input is slightly behind (rubato has
                // more input than time, so compress it). Same linear ramp
                // shape, but bounded by the larger SQUASH_THRESHOLD.
                let drift = requested_start_pts.saturating_sub(input_start_pts);
                let ratio = drift.as_secs_f64() / SQUASH_THRESHOLD.as_secs_f64();

                // multiply by 2.0 so max resampling is reached at the half point
                // of the squash limit
                self.set_resample_ratio_relative(1.0 - (2.0 * MAX_STRETCH_RATIO * ratio));
                trace!(ratio, ?drift, "Input buffer ahead, squashing");
            } else {
                // === DROP ===
                // Input is too far behind to recover by squashing. Fast-
                // forward by draining the over-old samples from the front
                // of the input buffer.
                // TODO: handle discontinuity (same caveat as gap-fill — the
                // filter state is now stale relative to the post-drop
                // signal).
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

            // One rubato batch's worth of output frames lands in
            // `output_buffer`. Loop continues until we have enough.
            self.resample();
        }
        self.output_buffer.read_samples(batch_size)
    }

    /// Pre-first-resample synchronization. Runs until `resample()` flips
    /// `before_first_resample` to false. Three cases against the requested
    /// `pts_range`:
    ///
    /// 1. **Input is entirely in the future** (`pts_range.1 < input_start`):
    ///    return a fully-silent output batch directly. We don't engage the
    ///    resampler at all — there is nothing to align to yet, and engaging
    ///    rubato would burn the warmup against zeros.
    /// 2. **`pts_range` straddles `input_start`**: prepend zeros to the
    ///    input buffer so that `input_buffer_start_pts == pts_range.0`,
    ///    then return None to let `get_samples` proceed normally.
    /// 3. **Input has data older than `pts_range.0`**: drain the
    ///    too-old prefix so the first emitted sample lands on `pts_range.0`.
    ///
    /// Note: case 3 doesn't account for `original_output_delay` at all. After
    /// the drain, the first resample produces output starting roughly at
    /// `pts_range.0 + output_delay` (then the warmup is dropped via
    /// `samples_to_drop`, restoring alignment).
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
            // Pad input with silence so the buffer's effective start aligns
            // with the requested range start. After this, the first resample
            // in `get_samples` will produce a partial-silence-then-real-audio
            // batch starting at `pts_range.0`.
            let duration = input_buffer_start_pts.saturating_sub(pts_range.0);
            let samples = (duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
            let batch = match self.channels {
                AudioChannels::Mono => AudioSamples::Mono(vec![0.0; samples]),
                AudioChannels::Stereo => AudioSamples::Stereo(vec![(0.0, 0.0); samples]),
            };
            trace!(samples, ?duration, "Add zero samples before first resample");
            self.resampler_input_buffer.push_front(batch)
        } else if pts_range.0 > input_buffer_start_pts {
            // Drop too-old samples so the buffer starts at `pts_range.0`.
            let duration = pts_range.0.saturating_sub(input_buffer_start_pts);
            let samples = (duration.as_secs_f64() * self.input_sample_rate as f64) as usize;
            trace!(samples, ?duration, "Drain samples before first resample");
            self.resampler_input_buffer.drain_samples(samples);
        }

        None
    }

    /// Run rubato once. Produces exactly one output batch
    /// (`samples_in_batch` frames) into `resampler_output_buffer`, then
    /// moves it to `output_buffer`.
    ///
    /// If `resampler_input_buffer` doesn't have the input frames rubato
    /// needs for a full batch, we use rubato's `partial_len` indexing to ask
    /// it to consume only what's available — rubato pads the rest with zeros
    /// internally. After a partial run, the filter has effectively been
    /// re-warmed against zero-padding, so we re-arm `samples_to_drop` with
    /// the current `output_delay` so the *next* read discards the new
    /// warmup. This is also what causes audible glitches at gap boundaries
    /// (the TODO "handle discontinuity" in `get_samples`).
    fn resample(&mut self) {
        self.before_first_resample = false;
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
        let (consumed_samples, funcerated_samples) = match self.resampler.process_into_buffer(
            &self.resampler_input_buffer,
            &mut self.resampler_output_buffer,
            indexing.as_ref(),
        ) {
            Ok(result) => result,
            Err(err) => {
                // Hard failure path: emit silence rather than stalling the
                // mixer. We pretend the full output buffer was funcerated so
                // the caller can keep advancing.
                error!("Resampling error: {err}");
                self.resampler_output_buffer.fill_with(&0.0);
                (0, self.resampler_output_buffer.frames())
            }
        };

        self.resampler_input_buffer.drain_samples(consumed_samples);
        if funcerated_samples != self.resampler_output_buffer.frames() {
            error!(
                expected = self.resampler_output_buffer.frames(),
                actual = funcerated_samples,
                "Resampler funcerated wrong amount of samples"
            )
        }
        self.output_buffer
            .push_back(self.resampler_output_buffer.get_samples());

        // After a partial run rubato's internal state is effectively
        // re-warming, so the next batch will once again contain
        // `output_delay` worth of meaningless prefix to discard.
        if missing_input_samples > 0 {
            self.resampler_output_buffer.samples_to_drop = self.resampler.output_delay();
        }
    }
}

/// Fixed-size scratch buffer that rubato writes into.
///
/// The buffer's length is `samples_in_batch` (set at construction); each
/// `resample()` call overwrites its contents in full. The
/// `audioadapter::AdapterMut` impl below is what rubato calls into.
///
/// `samples_to_drop` is non-zero whenever the *next* read should skip a
/// leading prefix — set on construction (initial filter warmup) and again
/// after partial-resample runs (effective re-warming).
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

    /// Take a copy of the current buffer contents, skipping the first
    /// `samples_to_drop` frames if non-zero. Resets `samples_to_drop` to 0
    /// after a single read — repeat reads of the same buffer would not have
    /// the same skip applied.
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
