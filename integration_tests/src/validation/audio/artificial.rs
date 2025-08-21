use anyhow::{anyhow, Result};
use spectrum_analyzer::{
    error::SpectrumAnalyzerError,
    samples_fft_to_spectrum,
    scaling::{scale_20_times_log10, scale_to_zero_to_one, SpectrumScalingFunction},
    windows::hann_window,
    Frequency, FrequencyLimit, FrequencySpectrum, FrequencyValue,
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

    let mut working_buffer: Vec<(Frequency, FrequencyValue)> =
        vec![(0.0.into(), 0.0.into()); expected_spectrum_left.data().len()];

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
    let samples = hann_window(samples);
    samples_fft_to_spectrum(&samples, sample_rate, FrequencyLimit::All, None)
}

fn scale_fft_spectrum(
    spectrum: &mut FrequencySpectrum,
    scaler: Option<f32>,
    working_buffer: &mut [(Frequency, FrequencyValue)],
) -> Result<(), SpectrumAnalyzerError> {
    let scaling_fn: Box<SpectrumScalingFunction> = match scaler {
        Some(scaler) if scaler > 0.0 => Box::new(move |val, _info| val / scaler),
        _ => Box::new(scale_to_zero_to_one),
    };
    spectrum.apply_scaling_fn(&scaling_fn, working_buffer)?;
    spectrum.apply_scaling_fn(&scale_20_times_log10, working_buffer)?;
    Ok(())
}
