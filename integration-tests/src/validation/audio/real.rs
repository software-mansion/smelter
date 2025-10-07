use anyhow::Result;
use spectrum_analyzer::{
    Frequency, FrequencyLimit, FrequencySpectrum, FrequencyValue,
    error::SpectrumAnalyzerError,
    samples_fft_to_spectrum,
    scaling::{SpectrumScalingFunction, scale_20_times_log10, scale_to_zero_to_one},
    windows::hann_window,
};
use tracing::{error, trace};

use crate::{
    audio::{RealFrequencyTolerance, SamplingInterval},
    validation::audio::{Channel, calc_level, find_samples, split_samples},
};

pub fn validate(
    full_expected_samples: Vec<f32>,
    full_actual_samples: Vec<f32>,
    sample_rate: u32,
    sampling_intervals: Vec<SamplingInterval>,
    tolerance: RealFrequencyTolerance,
    allowed_failed_batches: u32,
) -> Result<()> {
    let mut failed_batches: u32 = 0;
    for interval in sampling_intervals {
        let expected_samples = find_samples(&full_expected_samples, interval);
        let actual_samples = find_samples(&full_actual_samples, interval);

        let (expected_result_left, expected_result_right, actual_result_left, actual_result_right) =
            analyze_samples(actual_samples, expected_samples, sample_rate)?;

        let left_result = AnalyzeResult::compare(
            &actual_result_left,
            &expected_result_left,
            &tolerance,
            interval.first_sample,
            Channel::Left,
        );
        let right_result = AnalyzeResult::compare(
            &actual_result_right,
            &expected_result_right,
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
        Err(anyhow::anyhow!("Test failed!"))
    }
}

#[derive(Debug)]
struct AnalyzeResult {
    average_level: f32,
    median_level: f32,
    max_frequency: f32,
    max_frequency_level: f32,
    frequency_resolution: f32,
    general_level: f64,
}

impl AnalyzeResult {
    fn new(spectrum: FrequencySpectrum, general_level: f64) -> Self {
        Self {
            average_level: spectrum.average().val(),
            median_level: spectrum.median().val(),
            max_frequency: spectrum.max().0.val(),
            max_frequency_level: spectrum.max().1.val(),
            frequency_resolution: spectrum.frequency_resolution(),
            general_level,
        }
    }

    fn compare(
        actual: &Self,
        expected: &Self,
        tolerance: &RealFrequencyTolerance,
        first_sample: usize,
        channel: Channel,
    ) -> Result<()> {
        let average_level_diff = f32::abs(actual.average_level - expected.average_level);
        let median_level_diff = f32::abs(actual.median_level - expected.median_level);
        let max_frequency_diff = f32::abs(actual.max_frequency - expected.max_frequency);
        let max_frequency_level_diff =
            f32::abs(actual.max_frequency_level - expected.max_frequency_level);
        let general_level_diff = f64::abs(actual.general_level - expected.general_level);

        let max_frequency_tolerance =
            expected.frequency_resolution * tolerance.max_frequency as f32 + 10e-5;

        let average_level_match = average_level_diff <= tolerance.average_level;
        let median_level_match = median_level_diff <= tolerance.median_level;
        let max_frequency_match = max_frequency_diff <= max_frequency_tolerance;
        let max_frequency_level_match = max_frequency_level_diff <= tolerance.max_frequency_level;
        let general_level_match = general_level_diff <= tolerance.general_level;

        trace!("Check for max Frequency disabled {max_frequency_match}");

        let audio_match = average_level_match
            && median_level_match
          // disable check for max Frequency
          //  && max_frequency_match
            && max_frequency_level_match
            && general_level_match;

        if audio_match {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Audio mismatch at sample {first_sample} on channel {channel}: actual = {actual:#?}, expected = {expected:#?}",
            ))
        }
    }
}

fn analyze_samples(
    actual_samples: Vec<f32>,
    expected_samples: Vec<f32>,
    sample_rate: u32,
) -> Result<(AnalyzeResult, AnalyzeResult, AnalyzeResult, AnalyzeResult)> {
    let (expected_samples_left, expected_samples_right) = split_samples(expected_samples);
    let (actual_samples_left, actual_samples_right) = split_samples(actual_samples);

    if !(expected_samples_left.len() == expected_samples_right.len()
        && actual_samples_left.len() == actual_samples_right.len()
        && actual_samples_left.len() == expected_samples_left.len())
    {
        return Err(anyhow::anyhow!("Samples lengths do not match!"));
    }

    let (expected_level_left, amplitude_left) = calc_level(&expected_samples_left, None);
    let (expected_level_right, amplitude_right) = calc_level(&expected_samples_right, None);
    let (actual_level_left, _) = calc_level(&actual_samples_left, Some(amplitude_left));
    let (actual_level_right, _) = calc_level(&actual_samples_right, Some(amplitude_right));

    let mut expected_spectrum_left = calc_fft(&expected_samples_left, sample_rate)?;
    let mut expected_spectrum_right = calc_fft(&expected_samples_right, sample_rate)?;
    let mut actual_spectrum_left = calc_fft(&actual_samples_left, sample_rate)?;
    let mut actual_spectrum_right = calc_fft(&actual_samples_right, sample_rate)?;

    let left_scaler = expected_spectrum_left.max().1.val();
    let right_scaler = expected_spectrum_right.max().1.val();

    // Expected and actual sample batches should be of equal length
    let mut working_buffer: Vec<(Frequency, FrequencyValue)> =
        vec![(0.0.into(), 0.0.into()); expected_spectrum_left.data().len()];

    scale_fft_spectrum(&mut expected_spectrum_left, None, &mut working_buffer)?;
    scale_fft_spectrum(&mut expected_spectrum_right, None, &mut working_buffer)?;
    scale_fft_spectrum(
        &mut actual_spectrum_left,
        Some(left_scaler),
        &mut working_buffer,
    )?;
    scale_fft_spectrum(
        &mut actual_spectrum_right,
        Some(right_scaler),
        &mut working_buffer,
    )?;

    let expected_result_left = AnalyzeResult::new(expected_spectrum_left, expected_level_left);
    let expected_result_right = AnalyzeResult::new(expected_spectrum_right, expected_level_right);
    let actual_result_left = AnalyzeResult::new(actual_spectrum_left, actual_level_left);
    let actual_result_right = AnalyzeResult::new(actual_spectrum_right, actual_level_right);

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

    // TODO: (@jbrs) Change this parameter to plain f32
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
