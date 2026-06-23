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
use std::sync::Arc;
use std::time::Duration;

use smelter_render::InputId;

use super::*;
use crate::{
    Ref,
    prelude::{AudioMixerStatsSender, StatsSender},
};

/// Disconnected stats sender for resampler unit tests: events go nowhere,
/// but the resampler still emits them through the real send path.
pub(super) fn mock_stats_sender() -> AudioMixerStatsSender {
    AudioMixerStatsSender::new(
        StatsSender::disconnected(),
        Ref::new(&InputId(Arc::from("mock"))),
    )
}

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
    //   Duration::from_nanos(123456789 % 999_000 + 1_000);
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

pub(super) fn silence_samples(rate: u32, duration: Duration) -> Vec<f64> {
    vec![0.0; (duration.as_secs_f64() * rate as f64).round() as usize]
}

/// Write concatenated sample slices to
/// `<integration-tests/test_workdir>/resampler_tests/<name>` as a 16-bit
/// PCM mono WAV. Samples are clamped to `[-1.0, 1.0]` and quantized to
/// `i16`. The directory is created on first call.
pub(super) fn dump_wav(chunks: &[&[f64]], rate: u32, name: &str) {
    use std::io::Write;
    const BITS: u16 = 16;
    const CHANNELS: u16 = 1;
    let byte_rate = rate * CHANNELS as u32 * (BITS as u32 / 8);
    let block_align = CHANNELS * (BITS / 8);
    let total_frames: u32 = chunks.iter().map(|c| c.len() as u32).sum();
    let data_bytes = total_frames * (BITS as u32 / 8) * CHANNELS as u32;
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
    for chunk in chunks {
        for &s in *chunk {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f64).round() as i16;
            f.write_all(&v.to_le_bytes()).unwrap();
        }
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
    func: Arc<dyn Fn(Duration) -> f64 + Send + Sync>,
    rate: u32,
}

impl SignalSource {
    pub fn new<F: Fn(Duration) -> f64 + Send + Sync + 'static>(rate: u32, func: F) -> Self {
        Self {
            func: Arc::new(func),
            rate,
        }
    }

    pub fn shifted(&self, offset: Duration) -> Self {
        let func = self.func.clone();
        Self {
            func: Arc::new(move |pts| func(pts + offset)),
            rate: self.rate,
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
/// - Carrier sweeps **2000 Hz → 200 Hz over 1s** (k = -1800 Hz/s). Fast
///   enough that a 1-sample shift produces a measurable phase mismatch;
///   starts high so the most distinctive part of the signal lines up with
///   the early PTS values most tests use.
/// - Envelope cycles **every 15ms between 0.5 and 1.0** — stays loud
///   throughout (no near-silent troughs).
///
/// **Valid usage:**
/// - PTS range: `[Duration::ZERO, Duration::from_secs(1))`. Past 1s the
///   signal definition is still well-formed, but the parameter choice was
///   tuned for that window — extend deliberately if you need more.
/// - Any output sample rate ≥ 8kHz keeps the carrier well below Nyquist.
/// - Don't use this for silence assertions; pair `silence()` with a
///   separate `SignalSource` for those windows.
pub(super) fn test_signal() -> impl Fn(Duration) -> f64 + 'static {
    am_chirp(2000.0, -1800.0, Duration::from_millis(15), 0.5, 1.0)
}

/// Same idea as [`test_signal`] but tuned for tests with PTS values up to 5s.
/// Carrier sweeps **2000 Hz → 200 Hz over 5s** (k = -360 Hz/s) — starts
/// high so the most distinctive part of the signal lines up with early PTS
/// values. Stays well below Nyquist at 48 kHz. Envelope cycles every 10ms
/// between 0.2 and 1.0.
pub(super) fn test_signal_5s() -> impl Fn(Duration) -> f64 + 'static {
    am_chirp(2000.0, -360.0, Duration::from_millis(10), 0.2, 1.0)
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

/// Assertion that a slice of resampler output reproduces the reference
/// signal at a known PTS alignment.
///
/// On mismatch, panics with a structured diagnosis: worst sample error,
/// RMS at nominal vs best nearby shift (±64 samples), and first
/// mismatching samples.
pub(super) struct SignalAssertion<'a> {
    pub output: &'a [f64],
    pub source: &'a SignalSource,
}

struct AssertParams {
    tolerance: f64,
    stretch: f64,
}

pub(super) struct SignalAssertionWithParams<'a> {
    inner: &'a SignalAssertion<'a>,
    params: AssertParams,
}

impl<'a> SignalAssertionWithParams<'a> {
    #[allow(dead_code)]
    pub fn tolerance(mut self, tolerance: f64) -> Self {
        self.params.tolerance = tolerance;
        self
    }

    /// Assert that the output is time-stretched by `ratio` relative to the
    /// reference signal. 1.05 means the output is 5% longer (plays slower),
    /// 0.95 means 5% shorter (plays faster). 1.0 (default) means no stretch.
    pub fn stretch(mut self, ratio: f64) -> Self {
        self.params.stretch = ratio;
        self
    }

    pub fn assert(&self) {
        self.inner.assert_impl(&self.params);
    }
}

impl<'a> SignalAssertion<'a> {
    const DEFAULT_PARAMS: AssertParams = AssertParams {
        tolerance: 0.01,
        stretch: 1.0,
    };

    pub fn tolerance(&'a self, tolerance: f64) -> SignalAssertionWithParams<'a> {
        let mut params = Self::DEFAULT_PARAMS;
        params.tolerance = tolerance;
        SignalAssertionWithParams {
            inner: self,
            params,
        }
    }

    #[allow(dead_code)]
    /// Assert that the output is time-stretched by `ratio` relative to the
    /// reference signal. 1.05 means the output is 5% longer (plays slower),
    /// 0.95 means 5% shorter (plays faster). 1.0 (default) means no stretch.
    pub fn stretch(&'a self, ratio: f64) -> SignalAssertionWithParams<'a> {
        let mut params = Self::DEFAULT_PARAMS;
        params.stretch = ratio;
        SignalAssertionWithParams {
            inner: self,
            params,
        }
    }

    pub fn assert(&self) {
        self.assert_impl(&Self::DEFAULT_PARAMS);
    }

    fn assert_impl(&self, params: &AssertParams) {
        let actual = self.output;
        let length = actual.len();
        let dur = Duration::from_secs_f64(length as f64 / self.source.rate as f64);

        tracing::debug!(
            samples = length,
            duration_ms = dur.as_secs_f64() * 1000.0,
            tolerance = params.tolerance,
            stretch = params.stretch,
            "SignalAssertion::assert"
        );

        let stretch = params.stretch;
        let func = self.source.func.clone();
        let source = SignalSource {
            func: Arc::new(move |pts| func(Duration::from_secs_f64(pts.as_secs_f64() / stretch))),
            rate: self.source.rate,
        };

        let expected: Vec<f64> = (0..length)
            .map(|i| source.sample_at(Duration::from_secs_f64(i as f64 / source.rate as f64)))
            .collect();

        let (max_err, max_err_idx) = max_abs_error(actual, &expected);
        if max_err <= params.tolerance {
            tracing::debug!(max_err, "SignalAssertion passed");
            return;
        }

        let rms_nominal = rms_at_shift(actual, &source, 0.0);
        let (best_shift_secs, rms_best_shift) =
            best_alignment_shift(actual, &source, Duration::from_micros(1000));
        let (best_stretch, rms_best_stretch) =
            best_alignment_stretch(actual, self.source, params.stretch);

        let best_shift_us = best_shift_secs * 1_000_000.0;
        let mut buf = String::new();
        let _ = writeln!(buf);
        let _ = writeln!(buf, "Resampler alignment mismatch");
        let _ = writeln!(buf, "  expected: {length} samples");
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
            "  RMS @ shift 0: {rms_nominal:.4}    RMS @ best shift {best_shift_us:+.1}us: {rms_best_shift:.4}",
        );
        let _ = writeln!(
            buf,
            "  RMS @ stretch {:.6}: {:.4}    RMS @ best stretch {:.6}: {:.4}",
            params.stretch, rms_nominal, best_stretch, rms_best_stretch
        );

        if best_shift_us.abs() > 1.0 && rms_best_shift * 4.0 < rms_nominal {
            let direction = if best_shift_us > 0.0 {
                "later than"
            } else {
                "earlier than"
            };
            let _ = writeln!(
                buf,
                "  diagnosis: signal IS present but lands {best_shift_us:+.0}us {direction} expected",
            );
        } else if (best_stretch - params.stretch).abs() > 1e-6
            && rms_best_stretch * 4.0 < rms_nominal
        {
            let _ = writeln!(
                buf,
                "  diagnosis: signal IS present but stretched by {:.6} instead of {:.6}",
                best_stretch, params.stretch
            );
        } else {
            let _ = writeln!(
                buf,
                "  diagnosis: no nearby shift/stretch improves materially — signal is corrupted"
            );
        }

        let _ = writeln!(buf, "  first ≤20 mismatches:");
        let mut shown = 0;
        for i in 0..length {
            if shown >= 20 {
                break;
            }
            let err = (actual[i] - expected[i]).abs();
            if err > params.tolerance {
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

fn rms_at_shift(actual: &[f64], source: &SignalSource, shift_secs: f64) -> f64 {
    let mut sum_sq = 0.0_f64;
    let mut count = 0_u64;
    for (i, &y) in actual.iter().enumerate() {
        let t_secs = i as f64 / source.rate as f64 + shift_secs;
        if t_secs < 0.0 {
            continue;
        }
        let err = y - source.sample_at(Duration::from_secs_f64(t_secs));
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
    search_radius: Duration,
) -> (f64, f64) {
    let mut best_shift_secs = 0.0_f64;
    let mut best_rms = f64::INFINITY;
    // Search in 1µs steps so resolution is rate-independent.
    let radius_us = 10 * search_radius.as_micros() as i64;
    for us in -radius_us..=radius_us {
        let shift_secs = us as f64 / 10_000_000.0;
        let r = rms_at_shift(actual, source, shift_secs);
        if r < best_rms {
            best_rms = r;
            best_shift_secs = shift_secs;
        }
    }
    (best_shift_secs, best_rms)
}

fn rms_at_stretch(actual: &[f64], source: &SignalSource, stretch: f64) -> f64 {
    let mut sum_sq = 0.0_f64;
    for (i, &y) in actual.iter().enumerate() {
        let t_secs = i as f64 / source.rate as f64 / stretch;
        let err = y - source.sample_at(Duration::from_secs_f64(t_secs));
        sum_sq += err * err;
    }
    (sum_sq / actual.len() as f64).sqrt()
}

fn best_alignment_stretch(actual: &[f64], source: &SignalSource, center: f64) -> (f64, f64) {
    let mut best_stretch = center;
    let mut best_rms = f64::INFINITY;
    let step = 0.001 / 100.0;
    for i in -100..=100_i32 {
        let s = center + i as f64 * step;
        let r = rms_at_stretch(actual, source, s);
        if r < best_rms {
            best_rms = r;
            best_stretch = s;
        }
    }
    (best_stretch, best_rms)
}
