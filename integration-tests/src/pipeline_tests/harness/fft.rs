//! FFT-based audio comparison.
//!
//! Picks contiguous sample windows from each side and compares their
//! frequency content rather than the raw samples — phase shifts (such
//! as those introduced by an OPUS round-trip) don't affect the
//! spectrum, so we don't get false negatives for streams that *sound*
//! identical but have drifted by a handful of samples.
//!
//! Two modes:
//!   - [`Mode::Artificial`] — designed for synthetic test fixtures
//!     where each batch should contain a small number of clean tones.
//!     Compares the strongest bins between expected and actual.
//!   - [`Mode::Real`] — for general signals (music, speech). Compares
//!     summary statistics of each batch's spectrum (average level,
//!     median level, max-bin level, overall loudness).

use std::{ops::Range, time::Duration};

use anyhow::Result;

use crate::audio_decoder::AudioSampleBatch;

mod artificial;
mod real;

/// Caller-tunable thresholds for the FFT comparison.
pub struct FftCompareConfig {
    /// Time ranges over which the comparison runs. Each range is
    /// chopped into `samples_per_batch`-sized batches and every batch
    /// is checked independently.
    pub intervals: Vec<Range<Duration>>,
    /// MUST be a power of two (FFT requirement). At a sample rate of
    /// 48 kHz, `samples_per_batch = 32_768` covers ~0.34 s per channel
    /// after stereo de-interleaving.
    pub samples_per_batch: usize,
    pub sample_rate: u32,
    pub mode: Mode,
    /// Number of (channel, batch) pairs allowed to fail before the
    /// whole comparison errors. Real-world tests usually need 0; OPUS
    /// transients near a stream boundary may need 1–2.
    pub allowed_failed_batches: u32,
    /// PTS tolerance for the lightweight "do batch timestamps line
    /// up?" pre-check. Defaults to 20 ms.
    pub pts_offset: Duration,
}

impl FftCompareConfig {
    pub fn artificial(intervals: Vec<Range<Duration>>) -> Self {
        Self {
            intervals,
            samples_per_batch: 32_768,
            sample_rate: 48_000,
            mode: Mode::Artificial(ArtificialTolerance::default()),
            allowed_failed_batches: 0,
            pts_offset: Duration::from_millis(20),
        }
    }

    pub fn real(intervals: Vec<Range<Duration>>) -> Self {
        Self {
            intervals,
            samples_per_batch: 32_768,
            sample_rate: 48_000,
            mode: Mode::Real(RealTolerance::default()),
            allowed_failed_batches: 0,
            pts_offset: Duration::from_millis(20),
        }
    }
}

pub enum Mode {
    Real(RealTolerance),
    Artificial(ArtificialTolerance),
}

pub struct RealTolerance {
    /// Tolerance of max frequency, in multiples of the FFT's frequency
    /// resolution.
    pub max_frequency: u32,
    pub max_frequency_level: f32,
    pub average_level: f32,
    pub median_level: f32,
    pub general_level: f64,
}

impl Default for RealTolerance {
    fn default() -> Self {
        Self {
            max_frequency: 1,
            max_frequency_level: 3.0,
            average_level: 5.0,
            median_level: 5.0,
            general_level: 3.0,
        }
    }
}

pub struct ArtificialTolerance {
    /// Tolerance on per-bin frequency level (raw FFT magnitude units).
    /// Tuned to the amplitudes in `generate_frequencies.rs`; bump if
    /// you change those.
    pub frequency_level: f32,
}

impl Default for ArtificialTolerance {
    fn default() -> Self {
        Self {
            frequency_level: 40.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Channel {
    Left,
    Right,
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Left => "Left",
            Self::Right => "Right",
        };
        write!(f, "{msg}")
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SamplingInterval {
    pub first_sample: usize,
    pub samples: usize,
}

impl SamplingInterval {
    /// Carve `time_range` into back-to-back power-of-two batches on a
    /// fixed `samples_per_batch` grid. Each batch's `first_sample` is
    /// snapped to the nearest grid point at or below the range start
    /// so adjacent ranges line up cleanly.
    pub fn from_range(
        time_range: &Range<Duration>,
        sample_rate: u32,
        samples_per_batch: usize,
    ) -> Vec<Self> {
        let start_pts = time_range.start;
        let end_pts = time_range.end;
        if end_pts < start_pts {
            return vec![];
        }

        let time_per_batch = Duration::from_secs_f64(samples_per_batch as f64 / sample_rate as f64);

        let mut first_sample = (start_pts.as_secs_f64() * sample_rate as f64
            / samples_per_batch as f64) as usize
            * samples_per_batch;

        let mut intervals = vec![];
        loop {
            let pts = start_pts + intervals.len() as u32 * time_per_batch;
            if pts >= end_pts {
                break;
            }
            intervals.push(Self {
                first_sample,
                samples: samples_per_batch,
            });
            first_sample += samples_per_batch;
        }
        intervals
    }
}

/// Run the FFT comparison. Decodes-side gap and artifact analysis is
/// done elsewhere in [`super::audio`] — this is purely the spectrum
/// check.
pub fn compare(
    expected: &[AudioSampleBatch],
    actual: &[AudioSampleBatch],
    config: &FftCompareConfig,
) -> Result<()> {
    // Pre-check: timestamps of overlapping batches should line up. A
    // large drift is a sign the streams aren't even close to aligned;
    // running the FFT check on top of that is mostly noise.
    for range in &config.intervals {
        let actual_pts = collect_pts(actual, range, config.pts_offset);
        let expected_pts = collect_pts(expected, range, config.pts_offset);
        compare_pts(&actual_pts, &expected_pts, config.pts_offset)?;
    }

    let intervals: Vec<SamplingInterval> = config
        .intervals
        .iter()
        .flat_map(|r| SamplingInterval::from_range(r, config.sample_rate, config.samples_per_batch))
        .collect();

    let expected_samples: Vec<f32> = expected.iter().flat_map(|s| s.samples.clone()).collect();
    let actual_samples: Vec<f32> = actual.iter().flat_map(|s| s.samples.clone()).collect();

    match &config.mode {
        Mode::Real(tolerance) => real::validate(
            expected_samples,
            actual_samples,
            config.sample_rate,
            intervals,
            tolerance,
            config.allowed_failed_batches,
        ),
        Mode::Artificial(tolerance) => artificial::validate(
            expected_samples,
            actual_samples,
            config.sample_rate,
            intervals,
            tolerance,
            config.allowed_failed_batches,
        ),
    }
}

fn collect_pts(
    batches: &[AudioSampleBatch],
    range: &Range<Duration>,
    tolerance: Duration,
) -> Vec<Duration> {
    let lower = range.start.saturating_sub(tolerance);
    let higher = range.end + tolerance;
    let widened = lower..higher;
    batches
        .iter()
        .filter(|s| widened.contains(&s.pts))
        .map(|s| s.pts)
        .collect()
}

fn compare_pts(actual: &[Duration], expected: &[Duration], tolerance: Duration) -> Result<()> {
    for (a, e) in actual.iter().zip(expected.iter()) {
        let diff = if a >= e { *a - *e } else { *e - *a };
        if diff > tolerance {
            anyhow::bail!(
                "audio fft: pts drift exceeds tolerance — actual={:.4}s expected={:.4}s",
                a.as_secs_f64(),
                e.as_secs_f64()
            );
        }
    }
    Ok(())
}

pub(super) fn find_samples(samples: &[f32], interval: SamplingInterval) -> Vec<f32> {
    let first = interval.first_sample;
    let last = interval.first_sample + interval.samples;
    if first >= samples.len() {
        vec![0.0; interval.samples]
    } else if last > samples.len() {
        let mut batch = samples[first..].to_vec();
        batch.resize(interval.samples, 0.0);
        batch
    } else {
        samples[first..last].to_vec()
    }
}

pub(super) fn split_samples(samples: Vec<f32>) -> (Vec<f32>, Vec<f32>) {
    let left = samples.iter().step_by(2).copied().collect::<Vec<_>>();
    let right = samples
        .iter()
        .skip(1)
        .step_by(2)
        .copied()
        .collect::<Vec<_>>();
    (left, right)
}

/// Volume in dBFS. The 0 dB reference is `amplitude` if provided, else
/// the batch's own peak — the caller can pin the actual side to the
/// expected side's amplitude so the levels are comparable.
pub(super) fn calc_level(samples: &[f32], amplitude: Option<f64>) -> (f64, f64) {
    let max_sample = samples.iter().map(|s| s.abs()).reduce(f32::max).unwrap() as f64;
    let amplitude = amplitude.unwrap_or(max_sample);
    if amplitude > 0.0 {
        let dbfs = 20.0 * f64::log10(max_sample / amplitude);
        (dbfs, max_sample)
    } else {
        (0.0, max_sample)
    }
}
