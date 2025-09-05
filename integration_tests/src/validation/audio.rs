use anyhow::Result;
use bytes::Bytes;
use std::{cmp::Ordering, fmt::Display, iter::zip, ops::Range, time::Duration};

use crate::{
    audio_decoder::{AudioChannels, AudioDecoder, AudioSampleBatch},
    find_packets_for_payload_type, unmarshal_packets,
};

mod artificial;
mod real;

#[derive(Debug)]
pub enum Channel {
    Left,
    Right,
}

#[derive(Debug)]
pub enum ValidationMode {
    Real,
    Artificial,
}

impl Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::Left => "Left",
            Self::Right => "Right",
        };
        write!(f, "{msg}")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SamplingInterval {
    pub first_sample: usize,
    pub samples: usize,
}

impl SamplingInterval {
    // Intervals returned by this function do not match time stamp exactly.
    // They usually are slightly longer, because interval must be split into
    // batches of 16384 samples.
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

        // It finds the sample that fits pts best
        // If it is not a multiple of samples_per_batch find the highest
        // multiple lower than current number to be the starting sample
        let mut first_sample = (start_pts.as_secs_f64() * sample_rate as f64
            / samples_per_batch as f64) as usize
            * samples_per_batch;

        let mut intervals = vec![];
        loop {
            let pts = start_pts + intervals.len() as u32 * time_per_batch;
            if pts >= end_pts {
                break;
            }

            intervals.push(SamplingInterval {
                first_sample,
                samples: samples_per_batch,
            });
            first_sample += samples_per_batch;
        }
        intervals
    }
}

pub struct AudioAnalyzeTolerance {
    pub frequency_tolerance: FrequencyTolerance,
    pub allowed_failed_batches: u32,
    pub offset: Duration,
}

impl Default for AudioAnalyzeTolerance {
    fn default() -> Self {
        Self {
            frequency_tolerance: FrequencyTolerance::Artificial(Default::default()),
            allowed_failed_batches: 0,
            offset: Duration::from_millis(20),
        }
    }
}

pub enum FrequencyTolerance {
    Real(RealFrequencyTolerance),
    Artificial(ArtificialFrequencyTolerance),
}

impl FrequencyTolerance {
    pub fn real_tolerance(self) -> Option<RealFrequencyTolerance> {
        match self {
            Self::Real(t) => Some(t),
            _ => None,
        }
    }

    pub fn artificial_tolerance(self) -> Option<ArtificialFrequencyTolerance> {
        match self {
            Self::Artificial(t) => Some(t),
            _ => None,
        }
    }
}

pub struct RealFrequencyTolerance {
    /// Tolerance of max frequency. This value is the multiplier
    /// by which frequency resolution shall be multiplied when comparing values
    pub max_frequency: u32,
    pub max_frequency_level: f32,
    pub average_level: f32,
    pub median_level: f32,
    pub general_level: f64,
}

impl Default for RealFrequencyTolerance {
    fn default() -> Self {
        Self {
            // In case of spectral leaking
            max_frequency: 1,
            max_frequency_level: 3.0,
            average_level: 5.0,
            median_level: 5.0,
            general_level: 3.0,
        }
    }
}

pub struct ArtificialFrequencyTolerance {
    pub frequency_level: f32,
}

impl Default for ArtificialFrequencyTolerance {
    fn default() -> Self {
        Self {
            // WARN: This is dependent on the amplitude of input fixtups. If amplitudes are changed in
            // `generate_frequencies.rs` bin then this value should be adjusted.
            frequency_level: 40.0,
        }
    }
}

pub struct AudioValidationConfig {
    pub sampling_intervals: Vec<Range<Duration>>,
    pub channels: AudioChannels,
    pub sample_rate: u32,
    pub samples_per_batch: usize,
    pub tolerance: AudioAnalyzeTolerance,
}

impl Default for AudioValidationConfig {
    fn default() -> Self {
        Self {
            sampling_intervals: vec![Duration::from_secs(0)..Duration::from_secs(10)],
            channels: AudioChannels::Stereo,
            sample_rate: 48000,

            // It HAS TO be a power of 2 for FFT to work.
            // As 'channels' option is always set to stereo this will result in 16384 samples
            // per channel which is approx. 0.34s for the default sample rate.
            // This number MUST NOT exceed 32768 per channel.
            samples_per_batch: 32768,
            tolerance: AudioAnalyzeTolerance::default(),
        }
    }
}

pub fn validate(
    expected: &Bytes,
    actual: &Bytes,
    test_config: AudioValidationConfig,
    validation_mode: ValidationMode,
) -> Result<()> {
    let AudioValidationConfig {
        sampling_intervals: time_intervals,
        channels,
        sample_rate,
        samples_per_batch,
        tolerance,
    } = test_config;

    let expected_packets = unmarshal_packets(expected)?;
    let actual_packets = unmarshal_packets(actual)?;
    let expected_audio_packets = find_packets_for_payload_type(&expected_packets, 97);
    let actual_audio_packets = find_packets_for_payload_type(&actual_packets, 97);

    let mut expected_audio_decoder = AudioDecoder::new(sample_rate, channels)?;
    let mut actual_audio_decoder = AudioDecoder::new(sample_rate, channels)?;

    for packet in expected_audio_packets {
        expected_audio_decoder.decode(packet)?;
    }
    for packet in actual_audio_packets {
        actual_audio_decoder.decode(packet)?;
    }

    let expected_batches = expected_audio_decoder.take_samples();
    let actual_batches = actual_audio_decoder.take_samples();

    for range in &time_intervals {
        let actual_timestamps = find_timestamps(&actual_batches, range, tolerance.offset);
        let expected_timestamps = find_timestamps(&expected_batches, range, tolerance.offset);
        compare_timestamps(&actual_timestamps, &expected_timestamps, tolerance.offset)?;
    }

    let sampling_intervals = time_intervals
        .iter()
        .flat_map(|range| SamplingInterval::from_range(range, sample_rate, samples_per_batch))
        .collect::<Vec<_>>();

    let full_expected_samples = expected_batches
        .into_iter()
        .flat_map(|s| s.samples)
        .collect::<Vec<_>>();

    let full_actual_samples = actual_batches
        .into_iter()
        .flat_map(|s| s.samples)
        .collect::<Vec<_>>();

    let allowed_failed_batches = tolerance.allowed_failed_batches;

    match validation_mode {
        ValidationMode::Real => {
            let tolerance = tolerance.frequency_tolerance.real_tolerance().unwrap();
            real::validate(
                full_expected_samples,
                full_actual_samples,
                sample_rate,
                sampling_intervals,
                tolerance,
                allowed_failed_batches,
            )
        }
        ValidationMode::Artificial => {
            let tolerance = tolerance
                .frequency_tolerance
                .artificial_tolerance()
                .unwrap();
            artificial::validate(
                full_expected_samples,
                full_actual_samples,
                sample_rate,
                sampling_intervals,
                tolerance,
                allowed_failed_batches,
            )
        }
    }
}

fn find_timestamps(
    batches: &[AudioSampleBatch],
    time_range: &Range<Duration>,
    tolerance: Duration,
) -> Vec<Duration> {
    let lower = time_range.start.saturating_sub(tolerance);
    let higher = time_range.end + tolerance;
    let time_range = lower..higher;
    batches
        .iter()
        .filter(|s| time_range.contains(&s.pts))
        .map(|s| s.pts)
        .collect()
}

fn compare_timestamps(
    actual_timestamps: &[Duration],
    expected_timestamps: &[Duration],
    tolerance: Duration,
) -> Result<()> {
    for (actual, expected) in zip(actual_timestamps, expected_timestamps) {
        let diff = match actual.cmp(expected) {
            Ordering::Less | Ordering::Equal => *expected - *actual,
            Ordering::Greater => *actual - *expected,
        };
        if diff > tolerance {
            return Err(anyhow::anyhow!(
                "actual.pts = {}, expected.pts = {}",
                actual.as_secs_f64(),
                expected.as_secs_f64(),
            ));
        }
    }
    Ok(())
}

fn find_samples(samples: &[f32], interval: SamplingInterval) -> Vec<f32> {
    let first_sample = interval.first_sample;
    let last_sample = interval.first_sample + interval.samples;
    if first_sample >= samples.len() {
        vec![0.0; interval.samples]
    } else if last_sample > samples.len() {
        let mut batch = samples[first_sample..].to_vec();
        batch.resize(interval.samples, 0.0);
        batch
    } else {
        samples[first_sample..last_sample].to_vec()
    }
}

fn split_samples(samples: Vec<f32>) -> (Vec<f32>, Vec<f32>) {
    let samples_left = samples.iter().step_by(2).copied().collect::<Vec<_>>();

    let samples_right = samples
        .iter()
        .skip(1)
        .step_by(2)
        .copied()
        .collect::<Vec<_>>();
    (samples_left, samples_right)
}
// Calculates volume in dBFS (dB relevant to full scale) where point 0 is
// calculated based on amplitude of expected batch.
fn calc_level(samples: &[f32], amplitude: Option<f64>) -> (f64, f64) {
    // There should not be any NaN or Infinities and if there are the test should fail
    let max_sample = samples.iter().map(|s| s.abs()).reduce(f32::max).unwrap() as f64;
    let amplitude = match amplitude {
        Some(a) => a,
        None => max_sample,
    };
    if amplitude > 0.0 {
        let batch_dbfs = 20.0 * f64::log10(max_sample / amplitude);
        (batch_dbfs, max_sample)
    } else {
        (0.0, max_sample)
    }
}
