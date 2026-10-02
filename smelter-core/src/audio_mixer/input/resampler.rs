use std::time::Duration;

use audioadapter::{Adapter, AdapterMut};
use rubato::{
    FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction,
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
/// gaps. `write_batch` pads gaps of at least `SEAM_THRESHOLD` with zeros. Smaller gaps and
/// overlaps are left to drift control.
///
/// ## Outputs (what `get_samples` produces)
/// Exactly the number of frames at `output_sample_rate` that fit the requested `pts_range`,
/// padded with silence if the input cannot keep up. We assume that caller will request
/// pts ranges that is multiple of whole samples.
///
/// ## Data flow
/// 1. Incoming batches are appended to `resampler_input_buffer` (with gap padding and overlap
///    drop).
/// 2. `get_samples` runs `resample()` in a loop, each call moves a fixed `samples_in_batch`
///    worth of *output* frames from the rubato resampler into `output_buffer`, until
///    `output_buffer` has enough to satisfy the requested range.
/// 3. When real input runs out, rubato is fed zeros instead, so its filter state is never reset.
///    Only the leading frames of the very first resample correspond to filter warmup; they're
///    discarded via `ResamplerOutputBuffer::samples_to_drop`.
///
/// ## Holes
/// While there is no real input (`InputState::Hole`), the zeros fed to rubato carry no timing,
/// so the input timeline is kept on the output clock. New input is placed on it by its PTS in
/// `align_after_hole`: samples already in the past are dropped and samples in the future are
/// preceded by zeros. Real input next to a hole is faded over `FADE_DURATION`, except at the
/// stream start and at EOS.
///
/// ## Drift control
/// Input batches may arrive slightly early or late relative to the output timeline, so the
/// resampler adjusts its rate to compensate.
///
/// Two timestamps drive the stretch/squash decision in `correct_drift`:
/// - `requested_start_pts` — where the next output sample should land (in the mixing clock),
///   computed from `pts_range.0` plus what's already in `output_buffer`.
/// - `input_start_pts` — the mixing-clock PTS that the *next* output sample would actually
///   have if we ran rubato right now. Derived from `input_buffer_start_pts()` minus
///   `output_delay()`.
///
/// Their difference (the "drift") selects one of five branches:
/// - **gap-fill** — input is far behind: re-align it like after a hole.
/// - **stretch** — input is slightly behind: increase the resample ratio.
/// - **on-time** — drift within dead-band: ratio stays at 1.0.
/// - **squash** — input is slightly ahead: decrease the resample ratio.
/// - **drop** — input is far ahead: crossfade over excess input samples.
pub(super) struct InputResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: AudioChannels,

    /// Pending input PCM that hasn't been fed to rubato yet. Frames are consumed (drained) from
    /// the front each time `resample()` runs. Zeros are pushed to the back for gaps between
    /// batches or when input runs out, and to the front when input is aligned after a hole.
    resampler_input_buffer: AudioSamplesBuffer,
    /// Fixed-size scratch buffer that rubato writes one batch of output frames into. Owns its
    /// own `samples_to_drop` counter for warmup discarding.
    resampler_output_buffer: ResamplerOutputBuffer,

    /// Holds resampled output frames between rubato runs. We drain from this to satisfy each
    /// `get_samples(pts_range)` call.
    output_buffer: AudioSamplesBuffer,

    resampler: rubato::Async<f64>,

    /// PTS just past the last sample currently held in `resampler_input_buffer`. Combined with
    /// the buffer's frame count, it lets us compute `input_buffer_start_pts()` on demand.
    input_buffer_end_pts: Timestamp,

    state: InputState,
    /// Input track ended, its end is not faded out.
    eos: bool,
    /// `FADE_DURATION` in input samples.
    fade_len: usize,
    /// Number of zero input samples after which rubato's history contains only zeros.
    flush_len: usize,
}

#[derive(Debug, Clone, Copy)]
enum InputState {
    /// Real input is being resampled.
    Playing {
        /// Buffered input is about to run out and its end is already faded out. Input written
        /// before it runs out needs to be faded in.
        running_out: bool,
    },
    /// No real input: stream start, input ran out, or input needs re-aligning. Rubato is fed
    /// zeros until buffered input is aligned in `align_after_hole`.
    Hole {
        /// Fade in input after the hole. False on the stream start and after EOS.
        fade_in: bool,
        /// Zero samples fed to rubato since the hole started. After `flush_len` rubato would
        /// only output zeros, so it does not need to run.
        zeros_fed: usize,
    },
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

/// Minimal gap between consecutive batches that `write_batch` pads with zeros. Smaller gaps are
/// treated as timestamp jitter and left to drift control. Matches the shortest realistic packet
/// loss (10ms Opus packet).
const SEAM_THRESHOLD: Duration = Duration::from_millis(10);

/// Length of fades on real input next to zeros, and of the crossfade when dropping input.
/// Input is considered running out when less than this is buffered beyond the next resample, so
/// its end can still be faded out.
const FADE_DURATION: Duration = Duration::from_millis(5);

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

        let interpolation_params = Self::interpolation_params(input_sample_rate, output_sample_rate);
        let original_resampler_ratio = output_sample_rate as f64 / input_sample_rate as f64;
        let resampler = rubato::Async::<f64>::new_sinc(
            original_resampler_ratio,
            // Static upper bound on the *relative* ratio the resampler will accept at runtime.
            // Anything larger than this passed to `set_resample_ratio_relative` would be
            // rejected.
            1.0 + MAX_STRETCH_RATIO,
            interpolation_params,
            samples_in_batch,
            match channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            },
            FixedAsync::Output,
        )?;

        let mut resampler_output_buffer = ResamplerOutputBuffer::new(channels, samples_in_batch);
        // Number of *output* frames the rubato filter must "warm up" before it starts producing
        // meaningful samples. Dropping them shifts the produced timeline so the *first emitted
        // output sample* corresponds to the *first input sample* (rather than to
        // `-output_delay` worth of zero-padded warmup).
        resampler_output_buffer.samples_to_drop = resampler.output_delay();

        // Rubato keeps `2 * sinc_len` samples of history next to the samples of the current run.
        let flush_len = 2 * interpolation_params.sinc_len + resampler.input_frames_max();

        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            channels,

            resampler,
            resampler_input_buffer: AudioSamplesBuffer::new(channels),
            resampler_output_buffer,
            output_buffer: AudioSamplesBuffer::new(channels),

            input_buffer_end_pts: Timestamp::ZERO,

            // Fresh resampler has only zeros in its history
            state: InputState::Hole {
                fade_in: false,
                zeros_fed: flush_len,
            },
            eos: false,
            fade_len: (FADE_DURATION.as_secs_f64() * input_sample_rate as f64).round() as usize,
            flush_len,
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

    /// Input track ended, its end won't be faded out.
    pub fn mark_eos(&mut self) {
        self.eos = true;
    }

    fn input_buffer_start_pts(&self) -> Timestamp {
        self.input_buffer_end_pts
            - Duration::from_secs_f64(
                self.resampler_input_buffer.frames() as f64 / self.input_sample_rate as f64,
            )
    }

    /// Delay between the next input sample fed to rubato and the output sample produced from it.
    /// Warmup frames that are still going to be dropped don't count.
    fn output_delay(&self) -> Duration {
        let frames = self
            .resampler
            .output_delay()
            .saturating_sub(self.resampler_output_buffer.samples_to_drop);
        // rubato reports `output_delay` as a count of *output* frames
        Duration::from_secs_f64(frames as f64 / self.output_sample_rate as f64)
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

    /// Append a newly arrived input batch to `resampler_input_buffer`. A gap of at least
    /// `SEAM_THRESHOLD` after the buffered input is padded with zeros.
    pub fn write_batch(&mut self, batch: InputAudioSamples) {
        let (start_pts, end_pts) = batch.pts_range();
        trace!(
            ?start_pts,
            ?end_pts,
            len = batch.len(),
            "Resampler received a new batch"
        );
        // Input after EOS is a new track
        self.eos = false;

        // If samples overlap too much drop, for lower overlap than 80ms we let squashing handle
        // that.
        if start_pts + Duration::from_millis(80) < self.input_buffer_end_pts {
            debug!("Detected overlapping batches, dropping.");
            return;
        }

        let running_out = matches!(self.state, InputState::Playing { running_out: true });

        // With an empty buffer the batch is aligned in `get_samples`.
        let gap = start_pts - self.input_buffer_end_pts;
        let has_gap =
            self.resampler_input_buffer.frames() > 0 && gap >= Timestamp::from(SEAM_THRESHOLD);
        if has_gap {
            let gap_samples = (gap.as_secs_f64() * self.input_sample_rate as f64).round() as usize;
            debug!(?gap, gap_samples, "Gap between batches, padding with zeros.");
            if !running_out {
                self.resampler_input_buffer.fade_out_back(self.fade_len);
            }
            self.resampler_input_buffer.push_back_silence(gap_samples);
        }

        // This defines `input_buffer_end_pts()` results
        self.input_buffer_end_pts = end_pts;
        let batch_start = self.resampler_input_buffer.frames();
        self.resampler_input_buffer.push_back(batch.samples);

        if has_gap || running_out {
            self.resampler_input_buffer
                .fade_in(batch_start, self.fade_len);
        }
        if running_out {
            self.state = InputState::Playing { running_out: false };
        }
    }

    /// Produce exactly the number of output frames that fit `pts_range` at `output_sample_rate`.
    /// The decision-loop body runs once per `samples_in_batch` worth of output frames produced
    /// (because rubato emits a fixed-output-size batch per call).
    pub fn get_samples(&mut self, pts_range: (Timestamp, Timestamp)) -> AudioSamples {
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

            self.align_after_hole(requested_start_pts);
            match self.state {
                InputState::Playing { .. } => {
                    self.correct_drift(requested_start_pts);
                    if let InputState::Hole { .. } = self.state {
                        // Gap-fill, re-align input in the next iteration
                        continue;
                    }
                }
                InputState::Hole { zeros_fed, .. } if zeros_fed >= self.flush_len => {
                    // Rubato would only produce zeros. Skipping it is the same as feeding it
                    // zeros, because its history already contains only zeros.
                    self.output_buffer
                        .push_back_silence(batch_size - self.output_buffer.frames());
                    break;
                }
                InputState::Hole { .. } => self.set_resample_ratio_relative(1.0),
            }

            self.prepare_input();
            self.resample();

            if let InputState::Playing { running_out: true } = self.state
                && self.resampler_input_buffer.frames() == 0
            {
                debug!("Input ran out, resampling zeros");
                self.state = InputState::Hole {
                    fade_in: !self.eos,
                    zeros_fed: 0,
                };
            }
        }
        self.output_buffer.read_samples(batch_size)
    }

    /// Place buffered input on the output timeline after a hole. Zeros fed to rubato during the
    /// hole carry no timing, so the next input sample is always the one that lands on
    /// `requested_start_pts`.
    fn align_after_hole(&mut self, requested_start_pts: Timestamp) {
        let InputState::Hole { fade_in, .. } = self.state else {
            return;
        };
        let next_input_pts = requested_start_pts + self.output_delay();

        // If input buffer is empty or entirely in the past
        // Then drop it and keep the input timeline on the output clock
        if self.resampler_input_buffer.frames() == 0 || self.input_buffer_end_pts <= next_input_pts
        {
            if self.resampler_input_buffer.frames() > 0 {
                debug!(end_pts = ?self.input_buffer_end_pts, "Drop input that arrived too late");
                self.resampler_input_buffer.clear();
            }
            self.input_buffer_end_pts = next_input_pts;
            return;
        }

        let input_buffer_start_pts = self.input_buffer_start_pts();

        // If input buffer starts before the next input sample
        // Then drop samples that are already in the past
        if input_buffer_start_pts < next_input_pts {
            let duration = next_input_pts - input_buffer_start_pts;
            let samples = (duration.as_secs_f64() * self.input_sample_rate as f64).round() as usize;
            trace!(samples, ?duration, "Drop input samples after a hole");
            self.resampler_input_buffer.drain_samples(samples);
        }

        if fade_in {
            self.resampler_input_buffer.fade_in(0, self.fade_len);
        }

        // If input buffer starts after the next input sample
        // Then feed zeros until then
        if input_buffer_start_pts > next_input_pts {
            let duration = input_buffer_start_pts - next_input_pts;
            let samples = (duration.as_secs_f64() * self.input_sample_rate as f64).round() as usize;
            trace!(samples, ?duration, "Add zero samples after a hole");
            self.resampler_input_buffer.push_front_silence(samples);
        }

        self.state = InputState::Playing { running_out: false };
    }

    /// Adjust the resample ratio to the drift between input and output timelines.
    fn correct_drift(&mut self, requested_start_pts: Timestamp) {
        // PTS of the first timestamp that would be produced from resampler if current input
        // buffer was resampled. It takes into account that something is already in the
        // internal buffer.
        let input_start_pts = self.input_buffer_start_pts() - self.output_delay();

        if input_start_pts > requested_start_pts + STRETCH_THRESHOLD {
            // === GAP-FILL ===
            // `self.input_buffer_start_pts()` is too much in the future. Too much to try
            // to stretch, so re-align input like after a hole.
            let gap = input_start_pts - requested_start_pts;
            debug!(?gap, "Input buffer behind, re-aligning");
            self.state = InputState::Hole {
                fade_in: true,
                zeros_fed: 0,
            };
        } else if input_start_pts > requested_start_pts + SHIFT_THRESHOLD {
            // === STRETCH ===
            let drift = input_start_pts - requested_start_pts;
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
            let drift = requested_start_pts - input_start_pts;
            let drift_ratio = drift.as_secs_f64() / SQUASH_THRESHOLD.as_secs_f64();
            // multiply by 2.0 so max resampling is reached at the half point
            // of the squash limit
            let ratio = 2.0 * MAX_STRETCH_RATIO * drift_ratio;

            self.set_resample_ratio_relative(1.0 - ratio);
            trace!(ratio, ?drift, "Input buffer ahead, squashing");
        } else {
            // === DROP ===
            // `self.input_buffer_start_pts()` is too much "behind" to recover by squashing.
            let duration_to_drop = requested_start_pts - input_start_pts;
            let samples_to_drop = (duration_to_drop.as_secs_f64() * self.input_sample_rate as f64)
                .round() as usize;
            self.resampler_input_buffer
                .crossfade_drain(samples_to_drop, self.fade_len);
            self.set_resample_ratio_relative(1.0);
            debug!(
                samples_to_drop,
                ?duration_to_drop,
                "Input buffer ahead, dropping samples"
            );
        }
    }

    /// Make sure `resampler_input_buffer` has enough samples for the next `resample()`. When real
    /// input is about to run out, its end is faded out while it is still buffered, and the
    /// missing samples are filled with zeros.
    fn prepare_input(&mut self) {
        let needed = self.resampler.input_frames_next();
        let frames = self.resampler_input_buffer.frames();

        if let InputState::Playing { running_out } = &mut self.state
            && !*running_out
            && frames < needed + self.fade_len
        {
            debug!(frames, eos = self.eos, "Input is running out");
            if !self.eos {
                self.resampler_input_buffer.fade_out_back(self.fade_len);
            }
            *running_out = true;
        }

        if frames < needed {
            let zeros = needed - frames;
            self.resampler_input_buffer.push_back_silence(zeros);
            self.input_buffer_end_pts +=
                Duration::from_secs_f64(zeros as f64 / self.input_sample_rate as f64);
            if let InputState::Hole { zeros_fed, .. } = &mut self.state {
                *zeros_fed += zeros;
            }
        }
    }

    /// Run rubato once: feed `input_frames_next()` samples from `resampler_input_buffer` and push
    /// a batch of output frames onto `output_buffer`.
    fn resample(&mut self) {
        let (consumed_samples, generated_samples) = match self.resampler.process_into_buffer(
            &self.resampler_input_buffer,
            &mut self.resampler_output_buffer,
            None,
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
    }
}

/// Fixed-size scratch buffer that rubato writes into.
///
/// The buffer's length is `samples_in_batch` (set at construction); each `resample()` call
/// overwrites its contents in full. The `audioadapter::AdapterMut` impl below is what rubato
/// calls into.
///
/// `samples_to_drop` is non-zero whenever the next reads should skip a leading prefix — set on
/// construction (initial filter warmup). It can be larger than the buffer, then it spans
/// multiple reads.
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

    /// Take a copy of the current buffer contents, skipping up to `samples_to_drop` leading
    /// frames and decreasing `samples_to_drop` by the skipped amount.
    fn get_samples(&mut self) -> AudioSamples {
        if self.samples_to_drop == 0 {
            return self.buffer.clone();
        }
        let start = usize::min(self.samples_to_drop, self.buffer.len());
        self.samples_to_drop -= start;
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
