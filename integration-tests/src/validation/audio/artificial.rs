use std::{cmp::Ordering, iter::zip};

use anyhow::{anyhow, Result};
use spectrum_analyzer::{
    error::SpectrumAnalyzerError, samples_fft_to_spectrum, scaling::divide_by_N,
    windows::hann_window, Frequency, FrequencyLimit, FrequencySpectrum, FrequencyValue,
};
use tracing::error;

use crate::audio::{
    find_samples, split_samples, ArtificialFrequencyTolerance, Channel, SamplingInterval,
};

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

/// Number of top detected frequencies to be compared
const COMPARED_BINS: usize = 10;

// WARN: This is dependent on the amplitude of input fixtups. If amplitudes are changed in
// `generate_frequencies.rs` bin then this value should be adjusted.

/// All frequencies with magnitude below this value are considered noise and ignored in test.
const NOISE_VALUE: f32 = 200.0;

pub fn validate(
    full_expected_samples: Vec<f32>,
    full_actual_samples: Vec<f32>,
    sample_rate: u32,
    sampling_intervals: Vec<SamplingInterval>,
    tolerance: ArtificialFrequencyTolerance,
    allowed_failed_batches: u32,
) -> Result<()> {
    let mut failed_batches: u32 = 0;
    for interval in sampling_intervals {
        let expected_samples = find_samples(&full_expected_samples, interval);
        let actual_samples = find_samples(&full_actual_samples, interval);

        let spectrum = analyze_samples(actual_samples, expected_samples, sample_rate)?;

        let left_result = compare(
            &spectrum.actual_left,
            &spectrum.expected_left,
            &tolerance,
            interval.first_sample,
            Channel::Left,
        );

        let right_result = compare(
            &spectrum.actual_right,
            &spectrum.expected_right,
            &tolerance,
            interval.first_sample,
            Channel::Right,
        );

        if let Err(err) = left_result {
            error!("{err}");
            failed_batches += 1;
        }
        if let Err(err) = right_result {
            error!("{err}");
            failed_batches += 1;
        }
    }

    if failed_batches <= allowed_failed_batches {
        Ok(())
    } else {
        Err(anyhow!("Test failed"))
    }
}

fn compare(
    actual_bins: &[SpectrumBin],
    expected_bins: &[SpectrumBin],
    tolerance: &ArtificialFrequencyTolerance,
    first_sample: usize,
    channel: Channel,
) -> Result<()> {
    for (actual, expected) in zip(actual_bins, expected_bins) {
        let value_diff = (expected.frequency_value - actual.frequency_value).abs();
        let value_match = value_diff <= tolerance.frequency_level;

        let frequency_match = actual.frequency == expected.frequency;

        if !(frequency_match && value_match) {
            return Err(anyhow::anyhow!(
                "Audio mismatch at sample {first_sample} on channel {channel}: actual = {actual_bins:#?}, expected = {expected_bins:#?}",
            ));
        }
    }
    Ok(())
}

fn analyze_samples(
    actual_samples: Vec<f32>,
    expected_samples: Vec<f32>,
    sample_rate: u32,
) -> Result<AnalyzedSpectrum> {
    let (expected_samples_left, expected_samples_right) = split_samples(expected_samples);
    let (actual_samples_left, actual_samples_right) = split_samples(actual_samples);

    let mut expected_spectrum_left = calc_fft(&expected_samples_left, sample_rate)?
        .data()
        .to_vec();

    let mut expected_spectrum_right = calc_fft(&expected_samples_right, sample_rate)?
        .data()
        .to_vec();

    let mut actual_spectrum_left = calc_fft(&actual_samples_left, sample_rate)?.data().to_vec();

    let mut actual_spectrum_right = calc_fft(&actual_samples_right, sample_rate)?
        .data()
        .to_vec();

    let frequency_sort_cmp =
        |a: &(Frequency, FrequencyValue), b: &(Frequency, FrequencyValue)| -> Ordering {
            let negative = FrequencyValue::from(-1.0);

            let a = a.1 * negative;
            let b = b.1 * negative;

            a.cmp(&b)
        };

    expected_spectrum_left.sort_by(frequency_sort_cmp);
    expected_spectrum_right.sort_by(frequency_sort_cmp);
    actual_spectrum_left.sort_by(frequency_sort_cmp);
    actual_spectrum_right.sort_by(frequency_sort_cmp);

    let expected_result_left = prepare_spectrum_for_comparison(expected_spectrum_left);
    let expected_result_right = prepare_spectrum_for_comparison(expected_spectrum_right);
    let actual_result_left = prepare_spectrum_for_comparison(actual_spectrum_left);
    let actual_result_right = prepare_spectrum_for_comparison(actual_spectrum_right);

    Ok(AnalyzedSpectrum {
        actual_left: actual_result_left,
        expected_left: expected_result_left,
        actual_right: actual_result_right,
        expected_right: expected_result_right,
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

fn prepare_spectrum_for_comparison(
    spectrum_data: Vec<(Frequency, FrequencyValue)>,
) -> Vec<SpectrumBin> {
    spectrum_data
        .into_iter()
        .map(|(frequency, value)| SpectrumBin {
            frequency: frequency.val(),
            frequency_value: value.val(),
        })
        .filter(|bin| bin.frequency_value > NOISE_VALUE)
        .take(COMPARED_BINS)
        .collect()
}
