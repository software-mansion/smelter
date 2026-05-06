//! Reusable harness for `InputResampler` tests.
//!
//! Test files create one [`SignalSource`] per scenario and use it as the
//! single source of truth for both input batch generation and expected-output
//! reconstruction. [`assert_input_at_output_mono`] is the workhorse assertion
//! and produces a structured diagnosis on failure (worst sample, best ±N
//! alignment shift, first few mismatches) so off-by-N alignment bugs are
//! immediately distinguishable from real signal corruption.

use std::f64::consts::PI;
use std::fmt::Write;
use std::ops::Range;
use std::time::Duration;

use super::*;

// ============================ Signal source ============================

/// Pure function `pts -> sample` paired with a sample rate. Read-only:
/// `samples`/`batch`/`sample_at` can be called any number of times for any
/// range and always return the same data, so a single source can drive
/// *both* input generation *and* expected-output reconstruction in the same
/// test. Stored as a boxed trait object so test helpers can borrow
/// `&SignalSource` without leaking generic parameters through their
/// signatures.
pub(super) struct SignalSource {
    func: Box<dyn Fn(Duration) -> f64>,
    rate: u32,
}

impl SignalSource {
    pub fn new<F: Fn(Duration) -> f64 + 'static>(rate: u32, func: F) -> Self {
        Self {
            func: Box::new(func),
            rate,
        }
    }

    pub fn sample_at(&self, pts: Duration) -> f64 {
        (self.func)(pts)
    }

    /// Samples covering `[start, end)`. Frame count uses the same rounding
    /// rule as `InputResampler` (`((dur)*rate).round() as usize`).
    pub fn samples(&self, start: Duration, end: Duration) -> Vec<f64> {
        let frames =
            ((end.saturating_sub(start)).as_secs_f64() * self.rate as f64).round() as usize;
        (0..frames)
            .map(|i| self.sample_at(start + Duration::from_secs_f64(i as f64 / self.rate as f64)))
            .collect()
    }

    /// Build an `InputAudioSamples` batch covering `[start, start+duration)`.
    pub fn batch(&self, start: Duration, duration: Duration) -> InputAudioSamples {
        let samples = self.samples(start, start + duration);
        InputAudioSamples::new(AudioSamples::Mono(samples), start, self.rate)
    }
}

/// Linear chirp `sin(2π·(f0·t + 0.5·k·t²))`. Instantaneous frequency is
/// monotonic in `t`, so phase ↔ time has no aliasing — useful when later
/// tests need to detect drift unambiguously over long windows.
pub(super) fn chirp(f0_hz: f64, k_hz_per_s: f64) -> impl Fn(Duration) -> f64 + 'static {
    move |pts| {
        let t = pts.as_secs_f64();
        (2.0 * PI * (f0_hz * t + 0.5 * k_hz_per_s * t * t)).sin()
    }
}

/// Constant-zero signal. Combine with `SignalAssertion` to assert that a
/// given output window is silent.
pub(super) fn silence() -> impl Fn(Duration) -> f64 + 'static {
    |_| 0.0
}

// ============================ Assertions ============================

/// Assertion that a window of resampler output reproduces the reference
/// signal, with a known sample-by-sample alignment between source PTS and
/// output position.
///
/// In words: for every `i` in `0..output_window.len()`,
/// `output[output_window.start + i]` is expected to equal
/// `source.sample_at(source_pts_at_window_start + i / source.rate)` within
/// `tolerance`.
///
/// On mismatch, [`assert`](Self::assert) panics with a structured diagnosis
/// that includes:
/// - the worst single-sample error and where it is,
/// - the RMS at the nominal alignment and at the *best* nearby integer
///   shift (±64 samples), so a constant latency / drift shows up as
///   "signal IS present, just shifted by N samples (M ms)" rather than
///   getting lost in a sea of unhelpful sample diffs,
/// - the first few mismatching samples, for fine-grained inspection.
pub(super) struct SignalAssertion<'a> {
    /// Output buffer returned by `InputResampler::get_samples`.
    pub output: &'a [f64],
    /// Slice of `output` (by sample index) to compare against `source`.
    /// Use `0..output.len()` to check the whole buffer.
    pub output_window: Range<usize>,
    /// Reference signal that `output[output_window]` should reproduce.
    pub source: &'a SignalSource,
    /// PTS in `source` that is expected to align with the *first sample of
    /// the window*, i.e. with `output[output_window.start]`.
    pub source_pts_at_window_start: Duration,
    /// Per-sample max absolute error tolerated.
    pub tolerance: f64,
}

impl SignalAssertion<'_> {
    pub fn assert(&self) {
        assert!(
            self.output_window.end <= self.output.len(),
            "output_window {:?} is out of bounds for output.len()={}",
            self.output_window,
            self.output.len()
        );
        let length = self.output_window.len();
        let actual = &self.output[self.output_window.clone()];
        let expected = self.source.samples(
            self.source_pts_at_window_start,
            self.source_pts_at_window_start
                + Duration::from_secs_f64(length as f64 / self.source.rate as f64),
        );

        let (max_err, max_err_idx) = max_abs_error(actual, &expected);
        if max_err <= self.tolerance {
            return;
        }

        let rms_nominal = rms_at_shift(actual, self.source, self.source_pts_at_window_start, 0);
        let (best_shift, rms_best) =
            best_alignment_shift(actual, self.source, self.source_pts_at_window_start, 64);

        let mut buf = String::new();
        let _ = writeln!(buf);
        let _ = writeln!(
            buf,
            "Resampler alignment mismatch (tolerance = {:.4})",
            self.tolerance
        );
        let _ = writeln!(
            buf,
            "  expected: signal[{:?} .. {:?}] @ output[{} .. {}]",
            self.source_pts_at_window_start,
            self.source_pts_at_window_start
                + Duration::from_secs_f64(length as f64 / self.source.rate as f64),
            self.output_window.start,
            self.output_window.end
        );
        let _ = writeln!(
            buf,
            "  worst @ +{:>5} ({:?}):  actual={:+.4}  expected={:+.4}  err={:.4}",
            max_err_idx,
            Duration::from_secs_f64(max_err_idx as f64 / self.source.rate as f64),
            actual[max_err_idx],
            expected[max_err_idx],
            max_err
        );
        let _ = writeln!(
            buf,
            "  RMS @ shift 0: {:.4}    RMS @ best shift {:+}: {:.4}",
            rms_nominal, best_shift, rms_best
        );

        if best_shift != 0 && rms_best * 4.0 < rms_nominal {
            let dt =
                Duration::from_secs_f64(best_shift.unsigned_abs() as f64 / self.source.rate as f64);
            let direction = if best_shift > 0 {
                "later than"
            } else {
                "earlier than"
            };
            let _ = writeln!(
                buf,
                "  diagnosis: signal IS present but lands {:?} {} expected",
                dt, direction
            );
            let _ = writeln!(
                buf,
                "             (look for: filter latency, drift correction, off-by-N PTS math)"
            );
        } else {
            let _ = writeln!(
                buf,
                "  diagnosis: no nearby shift improves materially — signal is corrupted, not just shifted"
            );
        }

        let _ = writeln!(buf, "  first ≤10 mismatches:");
        let mut shown = 0;
        for i in 0..length {
            if shown >= 10 {
                break;
            }
            let err = (actual[i] - expected[i]).abs();
            if err > self.tolerance {
                let _ = writeln!(
                    buf,
                    "    [{:>5}] actual={:+.4}  expected={:+.4}  err={:.4}",
                    i, actual[i], expected[i], err
                );
                shown += 1;
            }
        }
        panic!("{}", buf);
    }
}

fn max_abs_error(a: &[f64], b: &[f64]) -> (f64, usize) {
    let mut max_err = 0.0_f64;
    let mut idx = 0_usize;
    for (i, (&x, &y)) in a.iter().zip(b).enumerate() {
        let e = (x - y).abs();
        if e > max_err {
            max_err = e;
            idx = i;
        }
    }
    (max_err, idx)
}

fn rms_at_shift(
    actual: &[f64],
    source: &SignalSource,
    input_start_pts: Duration,
    shift: i64,
) -> f64 {
    let mut sum_sq = 0.0_f64;
    let mut count = 0_u64;
    for (i, &y) in actual.iter().enumerate() {
        let target = i as i64 + shift;
        if target < 0 {
            continue;
        }
        let pts = input_start_pts + Duration::from_secs_f64(target as f64 / source.rate as f64);
        let err = y - source.sample_at(pts);
        sum_sq += err * err;
        count += 1;
    }
    if count == 0 {
        f64::INFINITY
    } else {
        (sum_sq / count as f64).sqrt()
    }
}

fn best_alignment_shift(
    actual: &[f64],
    source: &SignalSource,
    input_start_pts: Duration,
    radius: i64,
) -> (i64, f64) {
    let mut best = (0_i64, f64::INFINITY);
    for s in -radius..=radius {
        let r = rms_at_shift(actual, source, input_start_pts, s);
        if r < best.1 {
            best = (s, r);
        }
    }
    best
}
