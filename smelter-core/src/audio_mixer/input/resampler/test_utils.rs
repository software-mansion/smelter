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

// ============================ Logging ============================

/// Best-effort `tracing` subscriber init for tests. Reads `RUST_LOG`
/// (defaulting to `trace`) so individual tests can be run with
/// `RUST_LOG=trace cargo test ...` to surface the resampler's
/// `debug!`/`trace!` output. Safe to call from every test — repeated
/// calls in the same process are silently ignored.
pub(super) fn try_init_logger() {
    use std::sync::Once;
    use tracing_subscriber::{EnvFilter, fmt};

    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("trace"));
        let _ = fmt().with_env_filter(filter).with_test_writer().try_init();
    });
}

// ============================ PTS perturbation ============================

/// Sub-millisecond perturbation added to every test timestamp so the test
/// suite doesn't accidentally rely on round-millisecond inputs.
///
/// Re-randomized at *compile time* via `const-random` — every fresh
/// rebuild of this crate picks a new value in `[1µs, 1ms)`. To reproduce
/// a specific failing run, replace the call with a hard-coded literal.
pub(super) const D: Duration =
    Duration::from_nanos(const_random::const_random!(u64) % 999_000 + 1_000);

// ============================ WAV dump ============================

/// Subdirectory under `integration-tests/test_workdir` where resampler
/// test dumps land. The path is recomputed from `smelter-core`'s
/// `CARGO_MANIFEST_DIR` to avoid taking a build-time dep on
/// `integration-tests::paths`.
fn dump_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("integration-tests")
        .join("test_workdir")
        .join("resampler_tests")
}

/// Write `samples` to `<integration-tests/test_workdir>/resampler_tests/<name>`
/// as a 16-bit PCM mono WAV. Samples are clamped to `[-1.0, 1.0]` and
/// quantized to `i16`. The directory is created on first call.
///
/// `name` should be a bare filename like `"output.wav"`.
pub(super) fn dump_wav(samples: &[f64], rate: u32, name: &str) {
    use std::io::Write;
    const BITS: u16 = 16;
    const CHANNELS: u16 = 1;
    let byte_rate = rate * CHANNELS as u32 * (BITS as u32 / 8);
    let block_align = CHANNELS * (BITS / 8);
    let data_bytes = (samples.len() as u32) * (BITS as u32 / 8) * CHANNELS as u32;
    let riff_size = 36 + data_bytes;

    let dir = dump_dir();
    std::fs::create_dir_all(&dir).expect("create dump dir");
    let path = dir.join(name);

    let mut f = std::fs::File::create(&path).expect("create wav");
    f.write_all(b"RIFF").unwrap();
    f.write_all(&riff_size.to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&CHANNELS.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&block_align.to_le_bytes()).unwrap();
    f.write_all(&BITS.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_bytes.to_le_bytes()).unwrap();
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f64).round() as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
    }
}

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

/// Constant-zero signal. Combine with `SignalAssertion` to assert that a
/// given output window is silent.
pub(super) fn silence() -> impl Fn(Duration) -> f64 + 'static {
    |_| 0.0
}

/// Pre-configured [`am_chirp`] with parameters chosen for resampler tests:
///
/// - Carrier sweeps **2.2kHz → 200Hz over 5s** (k = -400 Hz/s). Slow
///   enough to stay visually smooth at every PTS in the range; fast
///   enough that a 1-sample shift produces a measurable phase mismatch.
///   Starts high so the most distinctive (high-frequency) part of the
///   signal lines up with the early PTS values most tests use.
/// - Envelope cycles **every 50ms between 0.5 and 1.0** — stays loud
///   throughout (no near-silent troughs).
///   The period is long enough that amplitude is near-monotonic across a
///   typical 20ms request window, which makes off-by-N sample shifts
///   visible in the envelope as well as the carrier phase.
///
/// **Valid usage:**
/// - PTS range: `[Duration::ZERO, Duration::from_secs(5))`. Past 5s the
///   signal definition is still well-formed, but the parameter choice was
///   tuned for that window — extend deliberately if you need more.
/// - Any output sample rate ≥ 8kHz keeps the carrier well below Nyquist
///   for the full 5s.
/// - Don't use this for silence assertions; pair `silence()` with a
///   separate `SignalSource` for those windows.
pub(super) fn test_signal() -> impl Fn(Duration) -> f64 + 'static {
    am_chirp(2200.0, -400.0, Duration::from_millis(30), 0.5, 1.0)
}

/// Linear chirp with cosine amplitude modulation. Carrier instantaneous
/// frequency is `f0_hz + k_hz_per_s · t`; amplitude oscillates between
/// `amp_lo` and `amp_hi` with period `am_period`.
///
/// Both phase and envelope are strictly defined functions of `t`, so any two
/// PTS values within the signal lifetime produce a distinguishable sample —
/// off-by-N alignment shows up as either a phase mismatch *or* an envelope
/// mismatch, whichever is more visible at that point in the sweep.
///
/// Recommended defaults for tests up to 5s at 48kHz:
/// `am_chirp(200.0, 4000.0, Duration::from_millis(10), 0.2, 1.0)`
/// (frequency 200Hz → 20200Hz over 5s — stays below Nyquist with margin;
/// envelope cycles every 10ms between 0.2 and 1.0).
pub(super) fn am_chirp(
    f0_hz: f64,
    k_hz_per_s: f64,
    am_period: Duration,
    amp_lo: f64,
    amp_hi: f64,
) -> impl Fn(Duration) -> f64 + 'static {
    let am_freq = 1.0 / am_period.as_secs_f64();
    let amp_mid = (amp_lo + amp_hi) * 0.5;
    let amp_swing = (amp_hi - amp_lo) * 0.5;
    move |pts| {
        let t = pts.as_secs_f64();
        let amp = amp_mid + amp_swing * (2.0 * PI * am_freq * t).cos();
        amp * (2.0 * PI * (f0_hz * t + 0.5 * k_hz_per_s * t * t)).sin()
    }
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
