use std::{cmp::Ordering, iter::zip};

use anyhow::{anyhow, Result};
use spectrum_analyzer::{
    error::SpectrumAnalyzerError, samples_fft_to_spectrum, scaling::SpectrumScalingFunction,
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

/// Number of top detected frequencies to be compared
const COMPARED_BINS: usize = 10;

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

        let (
            expected_samples_left,
            expected_samples_right,
            actual_samples_left,
            actual_samples_right,
        ) = analyze_samples(actual_samples, expected_samples, sample_rate)?;

        let left_result = compare(
            &actual_samples_left,
            &expected_samples_left,
            &tolerance,
            interval.first_sample,
            Channel::Left,
        );

        let right_result = compare(
            &actual_samples_right,
            &expected_samples_right,
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

type AnalyzeResult = (
    Vec<SpectrumBin>,
    Vec<SpectrumBin>,
    Vec<SpectrumBin>,
    Vec<SpectrumBin>,
);

fn analyze_samples(
    actual_samples: Vec<f32>,
    expected_samples: Vec<f32>,
    sample_rate: u32,
) -> Result<AnalyzeResult> {
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

    let cmp = |a: &(Frequency, FrequencyValue), b: &(Frequency, FrequencyValue)| -> Ordering {
        let negative = FrequencyValue::from(-1.0);

        let a = a.1 * negative;
        let b = b.1 * negative;

        a.cmp(&b)
    };

    expected_spectrum_left.sort_by(cmp);
    expected_spectrum_right.sort_by(cmp);
    actual_spectrum_left.sort_by(cmp);
    actual_spectrum_right.sort_by(cmp);

    let expected_result_left = expected_spectrum_left
        .into_iter()
        .map(|(frequency, value)| SpectrumBin {
            frequency: frequency.val(),
            frequency_value: value.val(),
        })
        .filter(|bin| bin.frequency_value > 10.0)
        .take(COMPARED_BINS)
        .collect();

    let expected_result_right = expected_spectrum_right
        .into_iter()
        .map(|(frequency, value)| SpectrumBin {
            frequency: frequency.val(),
            frequency_value: value.val(),
        })
        .filter(|bin| bin.frequency_value > 10.0)
        .take(COMPARED_BINS)
        .collect();

    let actual_result_left = actual_spectrum_left
        .into_iter()
        .map(|(frequency, value)| SpectrumBin {
            frequency: frequency.val(),
            frequency_value: value.val(),
        })
        .filter(|bin| bin.frequency_value > 10.0)
        .take(COMPARED_BINS)
        .collect();

    let actual_result_right = actual_spectrum_right
        .into_iter()
        .map(|(frequency, value)| SpectrumBin {
            frequency: frequency.val(),
            frequency_value: value.val(),
        })
        .filter(|bin| bin.frequency_value > 10.0)
        .take(COMPARED_BINS)
        .collect();

    Ok((
        expected_result_left,
        expected_result_right,
        actual_result_left,
        actual_result_right,
    ))
}

fn calc_fft(samples: &[f32], sample_rate: u32) -> Result<FrequencySpectrum, SpectrumAnalyzerError> {
    let fft_scaler: Box<SpectrumScalingFunction> = Box::new(|fr_val, stats| {
        let max_val = stats.max;
        if max_val == 0.0 {
            0.0
        } else {
            100.0 * fr_val / max_val
        }
    });

    let samples = hann_window(samples);
    samples_fft_to_spectrum(&samples, sample_rate, FrequencyLimit::All, None)
}
