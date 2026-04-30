use std::{cmp::Ordering, iter::zip};

use anyhow::{Result, anyhow};
use spectrum_analyzer::{
    Frequency, FrequencyLimit, FrequencySpectrum, FrequencyValue, error::SpectrumAnalyzerError,
    samples_fft_to_spectrum, scaling::divide_by_N, windows::hann_window,
};
use tracing::error;

use super::{ArtificialTolerance, Channel, SamplingInterval, find_samples, split_samples};

#[derive(Debug)]
struct SpectrumBin {
    frequency: f32,
    frequency_value: f32,
}

struct AnalyzedSpectrum {
    actual_left: Vec<SpectrumBin>,
    expected_left: Vec<SpectrumBin>,
    actual_right: Vec<SpectrumBin>,
    expected_right: Vec<SpectrumBin>,
}

/// Number of top detected frequencies to be compared.
const COMPARED_BINS: usize = 10;

/// Frequencies with magnitude below this are considered noise. Tuned
/// to the amplitudes in `generate_frequencies.rs`.
const NOISE_VALUE: f32 = 200.0;

pub(super) fn validate(
    expected: Vec<f32>,
    actual: Vec<f32>,
    sample_rate: u32,
    intervals: Vec<SamplingInterval>,
    tolerance: &ArtificialTolerance,
    allowed_failed_batches: u32,
) -> Result<()> {
    let mut failed_batches: u32 = 0;
    for interval in intervals {
        let expected_samples = find_samples(&expected, interval);
        let actual_samples = find_samples(&actual, interval);

        let spectrum = analyze(actual_samples, expected_samples, sample_rate)?;

        if let Err(err) = compare(
            &spectrum.actual_left,
            &spectrum.expected_left,
            tolerance,
            interval.first_sample,
            Channel::Left,
        ) {
            error!("{err}");
            failed_batches += 1;
        }
        if let Err(err) = compare(
            &spectrum.actual_right,
            &spectrum.expected_right,
            tolerance,
            interval.first_sample,
            Channel::Right,
        ) {
            error!("{err}");
            failed_batches += 1;
        }
    }

    if failed_batches <= allowed_failed_batches {
        Ok(())
    } else {
        Err(anyhow!(
            "audio fft (artificial): {failed_batches} batch(es) failed (allowed {allowed_failed_batches})"
        ))
    }
}

fn compare(
    actual_bins: &[SpectrumBin],
    expected_bins: &[SpectrumBin],
    tolerance: &ArtificialTolerance,
    first_sample: usize,
    channel: Channel,
) -> Result<()> {
    for (actual, expected) in zip(actual_bins, expected_bins) {
        let value_diff = (expected.frequency_value - actual.frequency_value).abs();
        let value_match = value_diff <= tolerance.frequency_level;
        let frequency_match = actual.frequency == expected.frequency;

        if !(frequency_match && value_match) {
            return Err(anyhow!(
                "Audio mismatch at sample {first_sample} on channel {channel} \
                 (value_diff: {value_diff}, tolerance: {}, frequency_diff: {})",
                tolerance.frequency_level,
                actual.frequency - expected.frequency
            ));
        }
    }
    Ok(())
}

fn analyze(actual: Vec<f32>, expected: Vec<f32>, sample_rate: u32) -> Result<AnalyzedSpectrum> {
    let (expected_left, expected_right) = split_samples(expected);
    let (actual_left, actual_right) = split_samples(actual);

    let mut expected_spectrum_left = calc_fft(&expected_left, sample_rate)?.data().to_vec();
    let mut expected_spectrum_right = calc_fft(&expected_right, sample_rate)?.data().to_vec();
    let mut actual_spectrum_left = calc_fft(&actual_left, sample_rate)?.data().to_vec();
    let mut actual_spectrum_right = calc_fft(&actual_right, sample_rate)?.data().to_vec();

    let by_value_desc =
        |a: &(Frequency, FrequencyValue), b: &(Frequency, FrequencyValue)| -> Ordering {
            let neg = FrequencyValue::from(-1.0);
            (a.1 * neg).cmp(&(b.1 * neg))
        };
    expected_spectrum_left.sort_by(by_value_desc);
    expected_spectrum_right.sort_by(by_value_desc);
    actual_spectrum_left.sort_by(by_value_desc);
    actual_spectrum_right.sort_by(by_value_desc);

    Ok(AnalyzedSpectrum {
        actual_left: prepare(actual_spectrum_left),
        expected_left: prepare(expected_spectrum_left),
        actual_right: prepare(actual_spectrum_right),
        expected_right: prepare(expected_spectrum_right),
    })
}

fn calc_fft(samples: &[f32], sample_rate: u32) -> Result<FrequencySpectrum, SpectrumAnalyzerError> {
    let samples = hann_window(samples);
    samples_fft_to_spectrum(
        &samples,
        sample_rate,
        FrequencyLimit::All,
        Some(&divide_by_N),
    )
}

fn prepare(spectrum_data: Vec<(Frequency, FrequencyValue)>) -> Vec<SpectrumBin> {
    spectrum_data
        .into_iter()
        .map(|(f, v)| SpectrumBin {
            frequency: f.val(),
            frequency_value: v.val(),
        })
        .filter(|bin| bin.frequency_value > NOISE_VALUE)
        .take(COMPARED_BINS)
        .collect()
}
