use std::time::Duration;

use audioadapter::{Adapter, AdapterMut};
use rubato::{
    FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use tracing::{debug, error, trace, warn};

use crate::{
    AudioChannels, AudioSamples, Timestamp,
    audio_mixer::input::resampler::splice::CROSSFADE_DURATION, prelude::InputAudioSamples,
    utils::AudioSamplesBuffer,
};

use concealment::ConcealmentHistory;

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
/// gaps. `write_batch` handles gaps and overlaps of at least `SEAM_THRESHOLD`. Smaller ones are
/// left to drift control.
///
/// ## Outputs (what `get_samples` produces)
/// Exactly the number of frames at `output_sample_rate` that fit the requested `pts_range`,
/// padded with silence if the input cannot keep up. We assume that caller will request
/// pts ranges that is multiple of whole samples.
///
/// ## Data flow
/// 1. Incoming batches are appended to `resampler_input_buffer` (with gap handling and overlap
///    drop).
/// 2. `get_samples` runs `InnerResampler::resample` in a loop, each call moves a fixed
///    `samples_in_batch` worth of *output* frames from the rubato resampler into
///    `output_buffer`, until `output_buffer` has enough to satisfy the requested range.
/// 3. The leading frames of the very first resample (or first after discontinuity) correspond
///    to filter warmup (samples the resampler hasn't fully "seen" yet); they're discarded via
///    `ResamplerOutputBuffer::samples_to_drop`.
/// 4. When the input runs out, it is extended with concealment and flushed through the
///    resampler (`InnerResampler::flush`), which is a discontinuity.
///
/// ## Drift control
/// Input batches may arrive slightly early or late relative to the output timeline, so the
/// resampler adjusts its rate to compensate.
///
/// Two timestamps drive the stretch/squash decision in `get_samples`:
/// - `output_start_pts` — where the next output sample should land (in the mixing clock),
///   computed from `pts_range.0` plus what's already in `output_buffer`.
/// - `input_start_pts` — the mixing-clock PTS that the *next* output sample would actually
///   have if we ran rubato right now. Derived from `input_buffer_start_pts()` minus
///   `resampler.output_delay()`.
///
/// Their difference (the "drift") selects one of five branches:
/// - **gap-fill** — input is far behind: fade out the front of the input, flush and resync.
/// - **stretch** — input is slightly behind: increase the resample ratio.
/// - **on-time** — drift within dead-band: ratio stays at 1.0.
/// - **squash** — input is slightly ahead: decrease the resample ratio.
/// - **drop** — input is far ahead: discard excess input samples.
///
/// Note: because we make the decision per-resample-iteration (not per-batch), we can decide
/// to squash even if `resampler_input_buffer` doesn't yet contain a full batch — the flush path
/// handles that.
pub(super) struct InputResampler {
    channels: AudioChannels,

    /// Pending input PCM that hasn't been fed to rubato yet. Frames are consumed (drained) from
    /// the front each time `resampler` runs. May also have zeros pushed to the front (resync
    /// after discontinuity), concealment pushed to the back (before flush on discontinuity),
    /// concealment and zeros pushed to the back (padding a gap while resyncing) or samples
    /// dropped from the front (drop branch).
    resampler_input_buffer: AudioSamplesBuffer,

    /// Holds resampled output frames between rubato runs. We drain from this to satisfy each
    /// `get_samples(pts_range)` call.
    output_buffer: AudioSamplesBuffer,

    resampler: InnerResampler,

    /// PTS just past the last sample currently held in `resampler_input_buffer`. Updated only in
    /// `write_batch`. Combined with the buffer's frame count, it lets us compute
    /// `input_buffer_start_pts()` on demand.
    input_buffer_end_pts: Timestamp,

    /// Tail of the real input written via `write_batch`, source for concealment. Cleared on every
    /// discontinuity.
    history: ConcealmentHistory,

    /// Synchronization gate. While true, `get_samples` either serves any frames already in
    /// `output_buffer` (padded with zeros) while there isn't enough input, or aligns the input
    /// buffer to the requested range (via `try_resync_after_discontinuity`) and *does not*
    /// engage the stretch/squash logic. Cleared once the gate passes; re-armed by
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

/// Drift dead-band. While `|input_start_pts - output_start_pts| < 2ms` we leave the resample
/// ratio at 1.0 — too small to be worth correcting, and constantly toggling the ratio is itself
/// a source of artifacts.
const SHIFT_THRESHOLD: Timestamp = Timestamp::from_millis(2);

/// Maximum drift we'll *squash* (input is ahead of requested) before switching to the hard-drop
/// branch. Asymmetric with `STRETCH_THRESHOLD` because squashing only discards data — it doesn't
/// fabricate any — so a generous limit here mostly trades latency for smoothness.
const SQUASH_THRESHOLD: Timestamp = Timestamp::from_millis(500);

/// Maximum drift we'll *stretch* (input is behind requested) before switching to the gap-fill
/// branch. Smaller than `SQUASH_THRESHOLD` because stretching beyond a small fraction of a frame
/// is audibly bad.
const STRETCH_THRESHOLD: Timestamp = Timestamp::from_millis(40);

/// Minimal gap or overlap between consecutive batches that `write_batch` treats as a
/// discontinuity. Smaller ones are treated as timestamp jitter and left to drift control. Matches
/// the shortest realistic packet loss (10ms Opus packet).
const SEAM_THRESHOLD: Duration = Duration::from_millis(10);

/// Maximal gap that `write_batch` pads while resyncing. The queue delivers batches at most ~80ms
/// past the requested range (`MIXER_STRETCH_BUFFER`), so buffered input ending this long before
/// a new batch is already in the past and would be dropped by the resync gate anyway, e.g.
/// decoder concealment of a long loss sent together with the data after it.
const MAX_PADDED_GAP: Duration = Duration::from_millis(500);

/// Lead of real input past the request end, beyond the one chunk that the run-out check needs,
/// required to restart after a discontinuity. Keeps a source that barely caught up from running
/// out again on the next call. A final segment of a stream that doesn't reach that far is lost.
const RESYNC_LEAD: Duration = Duration::from_millis(20);

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
        Ok(Self {
            channels,

            resampler: InnerResampler::new(input_sample_rate, output_sample_rate, channels)?,
            resampler_input_buffer: AudioSamplesBuffer::new(channels),
            output_buffer: AudioSamplesBuffer::new(channels),

            input_buffer_end_pts: Timestamp::ZERO,

            history: ConcealmentHistory::new(channels, input_sample_rate),

            needs_input_resync: true,
        })
    }

    pub fn channels(&self) -> AudioChannels {
        self.channels
    }

    pub fn input_sample_rate(&self) -> u32 {
        self.resampler.input_sample_rate
    }

    fn input_buffer_start_pts(&self) -> Timestamp {
        self.input_buffer_end_pts
            - Duration::from_secs_f64(
                self.resampler_input_buffer.frames() as f64
                    / self.resampler.input_sample_rate as f64,
            )
    }

    /// Append a newly arrived input batch to `resampler_input_buffer`. Gaps and overlaps are
    /// relative to the end of the buffered input, smaller than `SEAM_THRESHOLD` count as
    /// continuous. Larger overlap means that the batch starts a new timeline (e.g. after
    /// resynchronization upstream), it is a discontinuity like a gap.
    ///
    /// - Playing (`!needs_input_resync`):
    ///   - gap or overlap:
    ///     - flush with concealment existing buffer (switches needs_input_resync on)
    ///     - when resync happens it will fade-in other side of the gap, or drain the part of the
    ///       batch already covered by the flushed output and fade-in the rest
    ///   - otherwise: append
    /// - Resyncing (`needs_input_resync`):
    ///   - overlap + input_buffer, or gap over `MAX_PADDED_GAP`
    ///     - buffered input (if any) wasn't placed yet and is superseded by the batch or already
    ///       in the past, discard it and append the batch
    ///   - gap + input_buffer
    ///     - generate concealment and pad rest with zeros
    ///     - fade-in current batch before attaching it to input buffer
    ///   - gap + empty input_buffer
    ///     - no special logic because it will already be faded in when syncing, and
    ///       nothing to fade out
    ///   - otherwise: append, wait for sync (it will fade in buffer start)
    pub fn write_batch(&mut self, mut batch: InputAudioSamples) {
        let (start_pts, end_pts) = batch.pts_range();
        trace!(
            ?start_pts,
            ?end_pts,
            len = batch.len(),
            "Resampler received a new batch"
        );

        let is_buffer_empty = self.resampler_input_buffer.frames() == 0;
        // With empty buffer `input_buffer_end_pts` refers to input that is already gone, there is
        // nothing to overlap with.
        let has_overlap =
            !is_buffer_empty && start_pts + SEAM_THRESHOLD <= self.input_buffer_end_pts;
        let has_gap = start_pts >= self.input_buffer_end_pts + SEAM_THRESHOLD;
        let has_large_gap = start_pts > self.input_buffer_end_pts + MAX_PADDED_GAP;

        match self.needs_input_resync {
            false => {
                // If there is a gap or overlap flush everything before writing current batch.
                if has_gap || has_overlap {
                    // Negative on overlap.
                    let gap = start_pts - self.input_buffer_end_pts;
                    debug!(?gap, "Discontinuity between batches, flushing.");
                    // The end of the buffered input is concealed, start of the new one will be
                    // faded in because flushing switches needs_input_resync to true
                    self.flush_with_concealment();
                }
                // This defines `input_buffer_start_pts()` results
                self.input_buffer_end_pts = end_pts;
                self.history.push(&batch.samples);
                self.resampler_input_buffer.push_back(batch.samples);
            }
            true => {
                if has_overlap || has_large_gap {
                    // Buffered input wasn't placed yet. On overlap the batch starts a new timeline
                    // that supersedes it. After a large gap it is already in the past, padding
                    // would only produce silence for the gate to drain.
                    // Negative on overlap.
                    let gap = start_pts - self.input_buffer_end_pts;
                    debug!(
                        ?gap,
                        "Discontinuity between batches while resyncing, discarding buffer."
                    );
                    self.resampler_input_buffer.clear();
                    self.history.clear();

                    self.input_buffer_end_pts = end_pts;
                    self.history.push(&batch.samples);
                    self.resampler_input_buffer.push_back(batch.samples);
                } else if !is_buffer_empty && has_gap {
                    // Conceal the end of buffered input, then silence until the batch, which
                    // fades in.
                    let gap = start_pts - self.input_buffer_end_pts;
                    let gap_samples = (gap.as_secs_f64() * self.resampler.input_sample_rate as f64)
                        .round() as usize;
                    match self.history.conceal() {
                        Some(concealment) => {
                            let padding_samples = gap_samples.saturating_sub(concealment.len());
                            let padding = AudioSamples::zeros(self.channels, padding_samples);
                            self.resampler_input_buffer.push_back(concealment);
                            self.resampler_input_buffer.push_back(padding);
                        }
                        None => {
                            let padding = AudioSamples::zeros(self.channels, gap_samples);
                            self.resampler_input_buffer.push_back(padding);
                        }
                    };
                    debug!(?gap, "Gap between batches while resyncing, padding.");

                    self.input_buffer_end_pts = end_pts;
                    self.history.clear();
                    self.history.push(&batch.samples);
                    // This is not a start of the buffer, it won't get faded in when syncing
                    // so we need to fade it ourselves here
                    splice::fade_in(&mut batch.samples, self.resampler.input_sample_rate);
                    self.resampler_input_buffer.push_back(batch.samples);
                } else {
                    self.input_buffer_end_pts = end_pts;
                    self.history.push(&batch.samples);
                    self.resampler_input_buffer.push_back(batch.samples);
                }
            }
        }
    }

    /// Produce exactly the number of output frames that fit `pts_range` at `output_sample_rate`.
    pub fn get_samples(&mut self, pts_range: (Timestamp, Timestamp)) -> AudioSamples {
        let batch_size = ((pts_range.1 - pts_range.0).as_secs_f64()
            * self.resampler.output_sample_rate as f64)
            .round() as usize;

        // Output left in `output_buffer` covers the request, also defers resync to a call that
        // needs the input.
        if self.output_buffer.frames() >= batch_size {
            return self.output_buffer.read_samples(batch_size);
        }

        if self.needs_input_resync {
            // Output left in `output_buffer` (e.g. flushed before discontinuity) plays first,
            // input continues after it. Unlike drift control, output delay doesn't need to be
            // accounted for: rubato was reset, so its warmup samples are cut off.
            let output_buffer_delay = Duration::from_secs_f64(
                self.output_buffer.frames() as f64 / self.resampler.output_sample_rate as f64,
            );
            let start_pts = pts_range.0 + output_buffer_delay;
            if self.try_resync_after_discontinuity(start_pts, pts_range.1) {
                self.needs_input_resync = false;
            }
        }

        // Stops early on discontinuity (gap or input ran out), the next call resyncs.
        while !self.needs_input_resync && self.output_buffer.frames() < batch_size {
            self.resample_with_drift_control(pts_range);
        }
        // Pads with zeros if `output_buffer` doesn't have enough, e.g. after the input ran out.
        self.output_buffer.read_samples(batch_size)
    }

    /// Produce one rubato batch of output frames into `output_buffer`, correcting drift first.
    /// On discontinuity (gap or input ran out) flushes the resampler and re-arms
    /// `needs_input_resync` instead.
    fn resample_with_drift_control(&mut self, pts_range: (Timestamp, Timestamp)) {
        let output_start_pts = pts_range.0
            + Duration::from_secs_f64(
                self.output_buffer.frames() as f64 / self.resampler.output_sample_rate as f64,
            );
        let input_start_pts = self.input_buffer_start_pts() - self.resampler.output_delay();

        // `input_start_pts` and `output_start_pts` represent the same point in time, first sample
        // that should be produced in the next call. The only difference is that they are
        // calculated from 2 perspectives
        // - `output_start_pts` from requested range accounting for what is already in output
        //   buffer
        // - `input_start_pts` from last received chunk accounting for size of the input buffer and
        //   resampler delay
        //
        // Positive if input starts after the requested point (stretch), negative if before
        // (squash).
        let drift = input_start_pts - output_start_pts;

        if drift > STRETCH_THRESHOLD {
            // === GAP-FILL ===
            // Drift is too much to try to stretch, so treat it as a discontinuity: fade out the
            // front of the input into the gap and let the resync gate place the rest at its PTS.
            let crossfade_samples = (CROSSFADE_DURATION.as_secs_f64()
                * self.resampler.input_sample_rate as f64)
                .round() as usize;
            let fade_out_samples = self.resampler_input_buffer.read_samples(usize::min(
                crossfade_samples,
                self.resampler_input_buffer.frames(),
            ));

            let mut fade_out = AudioSamplesBuffer::from(fade_out_samples);
            splice::fade_out(&mut fade_out, self.resampler.input_sample_rate);
            let samples = self.resampler.flush(&mut fade_out);
            self.output_buffer.push_back(samples);
            self.reset_after_discontinuity();
            debug!(?drift, "Input buffer behind, restarting after the gap");
            return;
        } else if drift > SHIFT_THRESHOLD {
            // === STRETCH ===
            let drift_ratio = drift.as_secs_f64() / STRETCH_THRESHOLD.as_secs_f64();
            // multiply by 2.0 so max resampling is reached at the half point
            // of the stretch limit
            let ratio = 2.0 * MAX_STRETCH_RATIO * drift_ratio;

            self.resampler.set_resample_ratio_relative(1.0 + ratio);
            trace!(ratio, ?drift, "Input buffer behind, stretching");
        } else if drift > -SHIFT_THRESHOLD {
            // === ON-TIME (dead-band) ===
            // |drift| < SHIFT_THRESHOLD; leave the ratio alone.
            self.resampler.set_resample_ratio_relative(1.0);
            trace!("Input buffer on time");
        } else if drift > -SQUASH_THRESHOLD {
            // === SQUASH ===
            // `drift` is negative, so is the ratio.
            let drift_ratio = drift.as_secs_f64() / SQUASH_THRESHOLD.as_secs_f64();
            // multiply by 2.0 so max resampling is reached at the half point
            // of the squash limit
            let ratio = 2.0 * MAX_STRETCH_RATIO * drift_ratio;

            self.resampler.set_resample_ratio_relative(1.0 + ratio);
            trace!(ratio, ?drift, "Input buffer ahead, squashing");
        } else {
            // === DROP ===
            // `self.input_buffer_start_pts()` is too much "behind" to recover by squashing.
            // Skip input to catch up, joining both sides of the cut with a crossfade.
            let samples_to_drop =
                (drift.as_secs_f64().abs() * self.resampler.input_sample_rate as f64) as usize;
            match splice::drop_frames(
                &mut self.resampler_input_buffer,
                samples_to_drop,
                self.resampler.input_sample_rate,
            ) {
                Some(dropped) => {
                    debug!(
                        samples_to_drop,
                        dropped, "Input buffer ahead, dropping samples"
                    )
                }
                None => {
                    // Nothing to join with. Fade out, run-out path flushes the rest. History
                    // isn't contiguous with the faded out input anymore.
                    splice::fade_out(
                        &mut self.resampler_input_buffer,
                        self.resampler.input_sample_rate,
                    );
                    self.history.clear();
                    debug!(samples_to_drop, "Input buffer ahead, dropping all samples");
                }
            }
            self.resampler.set_resample_ratio_relative(1.0);
        }

        // Input runs out, extend it with concealment and flush all of it now. Output beyond
        // this request stays in `output_buffer`, `read_samples` pads the shortfall with
        // zeros. Leftover input would be misplaced by the gate, `input_buffer_end_pts`
        // doesn't cover concealment.
        if self.resampler_input_buffer.frames() < self.resampler.input_frames_next() {
            debug!(
                frames = self.resampler_input_buffer.frames(),
                "Input buffer too small, flushing"
            );
            self.flush_with_concealment();
            return;
        }

        // One rubato batch's worth of output frames lands in `output_buffer`.
        let samples = self.resampler.resample(&mut self.resampler_input_buffer);
        self.output_buffer.push_back(samples);
    }

    /// Pre-resample synchronization gate, called while `needs_input_resync` is set (initially,
    /// and after `reset_after_discontinuity`) and `output_buffer` doesn't cover the request.
    /// Aligns `resampler_input_buffer` so its earliest sample's PTS equals `start_pts`, where the
    /// next output sample lands, and fades it in. Returns false, leaving the input in place,
    /// until real input reaches `RESYNC_LEAD` past one chunk after `end_pts` (end of the
    /// requested range).
    fn try_resync_after_discontinuity(&mut self, start_pts: Timestamp, end_pts: Timestamp) -> bool {
        // If entire input buffer is in the past
        // Then drop it, caller outputs what is left in `output_buffer` (or zeros)
        if self.resampler_input_buffer.frames() > 0 && self.input_buffer_end_pts <= start_pts {
            trace!(
                end_pts = ?self.input_buffer_end_pts,
                "Drop input buffer on resync"
            );
            self.resampler_input_buffer.clear();
            self.history.clear();
            return false;
        }

        // If input buffer is empty
        // Then wait, caller outputs what is left in `output_buffer` (or zeros)
        if self.resampler_input_buffer.frames() == 0 {
            return false;
        }

        // If real input doesn't reach far enough past the request, it would run out right away
        // Then wait for more. Padding below doesn't count.
        let input_frame_duration = self.resampler.input_frame_duration();
        if self.input_buffer_end_pts < end_pts + input_frame_duration + RESYNC_LEAD {
            trace!(
                end_pts = ?self.input_buffer_end_pts,
                "Not enough input to resync, waiting"
            );
            return false;
        }

        let input_buffer_start_pts = self.input_buffer_start_pts();

        // If input buffer start before `start_pts`
        // Then drop samples that are too old, (new start will be faded in in next step)
        if start_pts > input_buffer_start_pts {
            let duration = start_pts - input_buffer_start_pts;
            let samples =
                (duration.as_secs_f64() * self.resampler.input_sample_rate as f64) as usize;
            trace!(samples, ?duration, "Drain samples before first resample");
            self.resampler_input_buffer.drain_samples(samples);
        }

        // fade in initial samples, it's important that this step is:
        // - after we cut off unnecessary samples
        // - before we pad extra zeros at the input buffer front
        {
            let crossfade = (CROSSFADE_DURATION.as_secs_f64()
                * self.resampler.input_sample_rate as f64)
                .round() as usize;
            let mut front = self
                .resampler_input_buffer
                .read_samples(usize::min(crossfade, self.resampler_input_buffer.frames()));
            splice::fade_in(&mut front, self.resampler.input_sample_rate);
            self.resampler_input_buffer.push_front(front);
        }

        // If input buffer starts after `start_pts`
        // Then pad with zeros at the front of input buffer (initial samples are already faded in)
        if start_pts < input_buffer_start_pts {
            let duration = input_buffer_start_pts - start_pts;
            let samples =
                (duration.as_secs_f64() * self.resampler.input_sample_rate as f64) as usize;
            trace!(
                samples,
                ?duration,
                "Add zero samples at the initial resample"
            );
            self.resampler_input_buffer
                .push_front(AudioSamples::zeros(self.channels, samples));
        }

        true
    }

    /// Extend the input with concealment, flush all of it through the resampler into
    /// `output_buffer` and reset after discontinuity.
    fn flush_with_concealment(&mut self) {
        if let Some(samples) = self.history.conceal() {
            trace!(len = samples.len(), "Concealing end of input");
            self.resampler_input_buffer.push_back(samples);
        }
        let samples = self.resampler.flush(&mut self.resampler_input_buffer);
        self.output_buffer.push_back(samples);
        self.reset_after_discontinuity();
    }

    /// Reset state that becomes invalid across an input discontinuity. Called after
    /// `resampler.flush`, which already reset rubato.
    /// - `needs_input_resync` — re-engage `try_resync_after_discontinuity` so the next
    ///   `get_samples` call realigns the (now empty) input buffer against the requested PTS
    ///   range before resampling.
    /// - `history` — next batch won't be contiguous with recorded input.
    fn reset_after_discontinuity(&mut self) {
        self.needs_input_resync = true;
        self.history.clear();
    }
}

/// Rubato wrapper with output aligned to input. Filter warmup at the start is dropped, and
/// `flush` emits the tail still held in the filter.
struct InnerResampler {
    rubato: rubato::Async<f64>,
    /// Fixed-size scratch buffer that rubato writes one batch of output frames into. Owns its
    /// own `samples_to_drop` counter for warmup discarding.
    output_buffer: ResamplerOutputBuffer,
    /// FIR filter delay of the resampler at construction time, as a Duration. Computed from
    /// `rubato.output_delay()` (a count of *output* frames) divided by `output_sample_rate`.
    /// Subtracted from `input_buffer_start_pts()` to get the PTS of the first warmup output
    /// sample in the input timeline.
    output_delay: Duration,
    channels: AudioChannels,
    input_sample_rate: u32,
    output_sample_rate: u32,
}

impl InnerResampler {
    fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        channels: AudioChannels,
    ) -> Result<Self, rubato::ResamplerConstructionError> {
        // Fixed *output* batch size for `FixedAsync::Output` mode: rubato will produce exactly
        // this many output frames per `process_into_buffer` call, consuming a variable number
        // of input frames to do so. At 48 kHz output, 256 frames ≈ 5.3 ms — small enough that
        // the stretch/squash decision in `get_samples` happens at fine granularity.
        let samples_in_batch = 256;

        let rubato = rubato::Async::<f64>::new_sinc(
            output_sample_rate as f64 / input_sample_rate as f64,
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
        // `output_buffer.samples_to_drop` below.
        let output_delay = rubato.output_delay();

        let mut output_buffer = ResamplerOutputBuffer::new(channels, samples_in_batch);
        // Tell the output buffer to discard its first `output_delay` frames on the next read.
        // This effectively shifts the produced timeline so the *first emitted output sample*
        // corresponds to the *first input sample* (rather than to `-output_delay` worth of
        // zero-padded warmup).
        output_buffer.samples_to_drop = output_delay;

        Ok(Self {
            rubato,
            output_buffer,
            // rubato reports `output_delay` as a count of *output* frames, so we divide by
            // `output_sample_rate` to get the physical delay in seconds. (Dividing by
            // `input_sample_rate` would over-shift by a factor of `ratio` whenever the rates
            // differ.)
            output_delay: Duration::from_secs_f64(output_delay as f64 / output_sample_rate as f64),
            channels,
            input_sample_rate,
            output_sample_rate,
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

    fn input_frames_next(&self) -> usize {
        self.rubato.input_frames_next()
    }

    /// Duration of `input_frames_next()` input frames.
    fn input_frame_duration(&self) -> Duration {
        Duration::from_secs_f64(self.input_frames_next() as f64 / self.input_sample_rate as f64)
    }

    fn output_delay(&self) -> Duration {
        self.output_delay
    }

    /// Adjust rubato's resample ratio by a multiplicative factor relative to the nominal
    /// `output_sample_rate / input_sample_rate`. `rel_ratio == 1.0` means "no correction".
    fn set_resample_ratio_relative(&mut self, rel_ratio: f64) {
        let rel_ratio = rel_ratio.clamp(1.0 / (1.0 + MAX_STRETCH_RATIO), 1.0 + MAX_STRETCH_RATIO);
        if let Err(err) = self.rubato.set_resample_ratio_relative(rel_ratio, true) {
            warn!(%err, "Failed to update resampler ratio.");
            let _ = self.rubato.set_resample_ratio_relative(1.0, true);
        }
    }

    /// Run rubato once on the front of `input`, which has to hold at least
    /// `input_frames_next()` frames. Consumed frames are drained from `input`.
    fn resample(&mut self, input: &mut AudioSamplesBuffer) -> AudioSamples {
        let (consumed, samples) = self.process_with_indexing(input, None);
        input.drain_samples(consumed);
        samples
    }

    /// Resample all of `input` (leaving it empty) and the tail still held in the filter, then
    /// reset. Returns output aligned with everything fed since the last reset.
    fn flush(&mut self, input: &mut AudioSamplesBuffer) -> AudioSamples {
        // Without ramping, so the output length below is exact.
        if let Err(err) = self.rubato.set_resample_ratio_relative(1.0, false) {
            warn!(%err, "Failed to update resampler ratio.");
        }
        // `input` plus the filter tail of already consumed input, minus warmup that is still to
        // be dropped.
        let output_len = ((input.frames() as f64 * self.rubato.resample_ratio()).round() as usize
            + self.rubato.output_delay())
        .saturating_sub(self.output_buffer.samples_to_drop);

        let mut output = AudioSamplesBuffer::new(self.channels);
        while output.frames() < output_len {
            // Once `input` is exhausted rubato is fed zeros.
            let indexing = Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: Some(input.frames()),
                active_channels_mask: None,
            };
            let (consumed, samples) = self.process_with_indexing(input, Some(&indexing));
            input.drain_samples(consumed);
            output.push_back(samples);
        }
        input.clear();

        self.rubato.reset();
        self.output_buffer.samples_to_drop = self.rubato.output_delay();
        output.read_samples(output_len)
    }

    fn process_with_indexing(
        &mut self,
        input: &AudioSamplesBuffer,
        indexing: Option<&Indexing>,
    ) -> (usize, AudioSamples) {
        let (consumed_samples, generated_samples) =
            match self
                .rubato
                .process_into_buffer(input, &mut self.output_buffer, indexing)
            {
                Ok(result) => result,
                Err(err) => {
                    // Hard failure path: emit silence rather than stalling the mixer. We pretend
                    // the full output buffer was generated so the caller can keep advancing.
                    error!("Resampling error: {err}");
                    self.output_buffer.fill_with(&0.0);
                    (0, self.output_buffer.frames())
                }
            };
        if generated_samples != self.output_buffer.frames() {
            error!(
                expected = self.output_buffer.frames(),
                actual = generated_samples,
                "Resampler generated wrong amount of samples"
            )
        }
        (consumed_samples, self.output_buffer.get_samples())
    }
}

/// Fixed-size scratch buffer that rubato writes into.
///
/// The buffer's length is `samples_in_batch` (set at construction); each rubato run overwrites
/// its contents in full. The `audioadapter::AdapterMut` impl below is what rubato calls into.
///
/// `samples_to_drop` is non-zero whenever the *next* read should skip a leading prefix — set on
/// construction (initial filter warmup) and again after flush (effective re-warming).
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

mod concealment;
mod splice;

#[cfg(test)]
mod equal_sample_rate_tests;
#[cfg(test)]
mod test_utils;
