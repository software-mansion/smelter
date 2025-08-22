use std::cmp::Ordering;

use anyhow::{anyhow, Result};
use spectrum_analyzer::{
    error::SpectrumAnalyzerError, samples_fft_to_spectrum, scaling::SpectrumDataStats,
    windows::hann_window, Frequency, FrequencyLimit, FrequencySpectrum, FrequencyValue,
};

use crate::audio::{find_samples, split_samples, ArtificialFrequencyTolerance, SamplingInterval};

type SpectrumBins = Vec<(Frequency, FrequencyValue)>;

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
    // let mut failed_batches: u32 = 0;
    for interval in sampling_intervals {
        let expected_samples = find_samples(&full_expected_samples, interval);
        let actual_samples = find_samples(&full_actual_samples, interval);

        let (expected_result_left, expected_result_right, actual_result_left, actual_result_right) =
            analyze_samples(actual_samples, expected_samples, sample_rate)?;

        // println!("Expected spectrum: {:#?}", expected_result_left.spectrum);
        println!("Actual spectrum: {:#?}", actual_result_left);
        println!("Expected spectrum: {:#?}", expected_result_left);
    }

    // This is just a placeholder #remove
    Err(anyhow!("Test failed"))
}

fn analyze_samples(
    actual_samples: Vec<f32>,
    expected_samples: Vec<f32>,
    sample_rate: u32,
) -> Result<(SpectrumBins, SpectrumBins, SpectrumBins, SpectrumBins)> {
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

    let expected_result_left = expected_spectrum_left[..COMPARED_BINS].to_vec();
    let expected_result_right = expected_spectrum_right[..COMPARED_BINS].to_vec();
    let actual_result_left = actual_spectrum_left[..COMPARED_BINS].to_vec();
    let actual_result_right = actual_spectrum_right[..COMPARED_BINS].to_vec();

    Ok((
        expected_result_left,
        expected_result_right,
        actual_result_left,
        actual_result_right,
    ))
}

fn calc_fft(samples: &[f32], sample_rate: u32) -> Result<FrequencySpectrum, SpectrumAnalyzerError> {
    let fft_scaler: Box<dyn Fn(f32, &SpectrumDataStats) -> f32> = Box::new(|fr_val, stats| {
        let max_val = stats.max;
        if max_val == 0.0 {
            0.0
        } else {
            100.0 * fr_val / max_val
        }
    });

    let samples = hann_window(samples);
    samples_fft_to_spectrum(
        &samples,
        sample_rate,
        FrequencyLimit::All,
        Some(&fft_scaler),
    )
}
