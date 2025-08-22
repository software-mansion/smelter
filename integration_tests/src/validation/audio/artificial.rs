use anyhow::{anyhow, Result};
use spectrum_analyzer::{
    error::SpectrumAnalyzerError, samples_fft_to_spectrum, scaling::SpectrumDataStats,
    windows::hann_window, FrequencyLimit, FrequencySpectrum,
};

use crate::audio::{find_samples, split_samples, AudioAnalyzeTolerance, SamplingInterval};

pub fn validate(
    full_expected_samples: Vec<f32>,
    full_actual_samples: Vec<f32>,
    sample_rate: u32,
    sampling_intervals: Vec<SamplingInterval>,
    tolerance: AudioAnalyzeTolerance,
    allowed_failed_batches: u32,
) -> Result<()> {
    // let mut failed_batches: u32 = 0;
    for interval in sampling_intervals {
        let expected_samples = find_samples(&full_expected_samples, interval);
        let actual_samples = find_samples(&full_actual_samples, interval);

        let (expected_result_left, expected_result_right, actual_result_left, actual_result_right) =
            analyze_samples(actual_samples, expected_samples, sample_rate)?;

        // println!("Expected spectrum: {:#?}", expected_result_left.spectrum);
        println!("Actual spectrum: {:#?}", actual_result_left.spectrum);
    }

    // This is just a placeholder #remove
    Err(anyhow!("Test failed"))
}

struct AnalyzeResult {
    spectrum: FrequencySpectrum,
}

impl AnalyzeResult {
    fn new(spectrum: FrequencySpectrum) -> Self {
        Self { spectrum }
    }
}

fn analyze_samples(
    actual_samples: Vec<f32>,
    expected_samples: Vec<f32>,
    sample_rate: u32,
) -> Result<(AnalyzeResult, AnalyzeResult, AnalyzeResult, AnalyzeResult)> {
    let (expected_samples_left, expected_samples_right) = split_samples(expected_samples);
    let (actual_samples_left, actual_samples_right) = split_samples(actual_samples);

    let mut expected_spectrum_left = calc_fft(&expected_samples_left, sample_rate)?;
    let mut expected_spectrum_right = calc_fft(&expected_samples_right, sample_rate)?;
    let mut actual_spectrum_left = calc_fft(&actual_samples_left, sample_rate)?;
    let mut actual_spectrum_right = calc_fft(&actual_samples_right, sample_rate)?;

    // scale_fft_spectrum(&mut actual_spectrum_left, None, &mut working_buffer)?;
    // scale_fft_spectrum(&mut actual_spectrum_right, None, &mut working_buffer)?;
    // scale_fft_spectrum(
    //     &mut actual_spectrum_left,
    //     Some(left_scaler),
    //     &mut working_buffer,
    // )?;
    // scale_fft_spectrum(
    //     &mut actual_spectrum_right,
    //     Some(right_scaler),
    //     &mut working_buffer,
    // )?;

    let expected_result_left = AnalyzeResult::new(expected_spectrum_left);
    let expected_result_right = AnalyzeResult::new(expected_spectrum_right);
    let actual_result_left = AnalyzeResult::new(actual_spectrum_left);
    let actual_result_right = AnalyzeResult::new(actual_spectrum_right);

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
        FrequencyLimit::Range(100.0, 1000.0),
        Some(&fft_scaler),
    )
}
