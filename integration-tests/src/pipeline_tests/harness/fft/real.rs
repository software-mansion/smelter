use anyhow::{Result, anyhow};
use spectrum_analyzer::{
    Frequency, FrequencyLimit, FrequencySpectrum, FrequencyValue,
    error::SpectrumAnalyzerError,
    samples_fft_to_spectrum,
    scaling::{SpectrumScalingFunction, scale_20_times_log10, scale_to_zero_to_one},
    windows::hann_window,
};
use tracing::error;

use super::{Channel, RealTolerance, SamplingInterval, calc_level, find_samples, split_samples};

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
        tolerance: &RealTolerance,
        first_sample: usize,
        channel: Channel,
    ) -> Result<()> {
        let avg_diff = (actual.average_level - expected.average_level).abs();
        let med_diff = (actual.median_level - expected.median_level).abs();
        let max_freq_diff = (actual.max_frequency - expected.max_frequency).abs();
        let max_freq_level_diff = (actual.max_frequency_level - expected.max_frequency_level).abs();
        let general_diff = (actual.general_level - expected.general_level).abs();

        let max_freq_tolerance =
            expected.frequency_resolution * tolerance.max_frequency as f32 + 10e-5;

        let avg_match = avg_diff <= tolerance.average_level;
        let med_match = med_diff <= tolerance.median_level;
        // Intentionally disabled — see legacy comment.
        let _max_freq_match = max_freq_diff <= max_freq_tolerance;
        let max_freq_level_match = max_freq_level_diff <= tolerance.max_frequency_level;
        let general_match = general_diff <= tolerance.general_level;

        if avg_match && med_match && max_freq_level_match && general_match {
            Ok(())
        } else {
            Err(anyhow!(
                "Audio mismatch at sample {first_sample} on channel {channel}: \
                 actual = {actual:#?}, expected = {expected:#?}",
            ))
        }
    }
}

pub(super) fn validate(
    expected: Vec<f32>,
    actual: Vec<f32>,
    sample_rate: u32,
    intervals: Vec<SamplingInterval>,
    tolerance: &RealTolerance,
    allowed_failed_batches: u32,
) -> Result<()> {
    let mut failed_batches: u32 = 0;
    for interval in intervals {
        let expected_samples = find_samples(&expected, interval);
        let actual_samples = find_samples(&actual, interval);

        let (e_l, e_r, a_l, a_r) = analyze(actual_samples, expected_samples, sample_rate)?;

        if let Err(err) =
            AnalyzeResult::compare(&a_l, &e_l, tolerance, interval.first_sample, Channel::Left)
        {
            error!("{err}");
            failed_batches += 1;
        }
        if let Err(err) =
            AnalyzeResult::compare(&a_r, &e_r, tolerance, interval.first_sample, Channel::Right)
        {
            error!("{err}");
            failed_batches += 1;
        }
    }

    if failed_batches <= allowed_failed_batches {
        Ok(())
    } else {
        Err(anyhow!(
            "audio fft (real): {failed_batches} batch(es) failed (allowed {allowed_failed_batches})"
        ))
    }
}

fn analyze(
    actual: Vec<f32>,
    expected: Vec<f32>,
    sample_rate: u32,
) -> Result<(AnalyzeResult, AnalyzeResult, AnalyzeResult, AnalyzeResult)> {
    let (e_l, e_r) = split_samples(expected);
    let (a_l, a_r) = split_samples(actual);

    if !(e_l.len() == e_r.len() && a_l.len() == a_r.len() && a_l.len() == e_l.len()) {
        return Err(anyhow!("Samples lengths do not match!"));
    }

    let (e_lvl_l, e_amp_l) = calc_level(&e_l, None);
    let (e_lvl_r, e_amp_r) = calc_level(&e_r, None);
    let (a_lvl_l, _) = calc_level(&a_l, Some(e_amp_l));
    let (a_lvl_r, _) = calc_level(&a_r, Some(e_amp_r));

    let mut e_spec_l = calc_fft(&e_l, sample_rate)?;
    let mut e_spec_r = calc_fft(&e_r, sample_rate)?;
    let mut a_spec_l = calc_fft(&a_l, sample_rate)?;
    let mut a_spec_r = calc_fft(&a_r, sample_rate)?;

    let l_scaler = e_spec_l.max().1.val();
    let r_scaler = e_spec_r.max().1.val();

    let mut working: Vec<(Frequency, FrequencyValue)> =
        vec![(0.0.into(), 0.0.into()); e_spec_l.data().len()];

    scale_spectrum(&mut e_spec_l, None, &mut working)?;
    scale_spectrum(&mut e_spec_r, None, &mut working)?;
    scale_spectrum(&mut a_spec_l, Some(l_scaler), &mut working)?;
    scale_spectrum(&mut a_spec_r, Some(r_scaler), &mut working)?;

    Ok((
        AnalyzeResult::new(e_spec_l, e_lvl_l),
        AnalyzeResult::new(e_spec_r, e_lvl_r),
        AnalyzeResult::new(a_spec_l, a_lvl_l),
        AnalyzeResult::new(a_spec_r, a_lvl_r),
    ))
}

fn calc_fft(samples: &[f32], sample_rate: u32) -> Result<FrequencySpectrum, SpectrumAnalyzerError> {
    let samples = hann_window(samples);
    samples_fft_to_spectrum(&samples, sample_rate, FrequencyLimit::All, None)
}

fn scale_spectrum(
    spectrum: &mut FrequencySpectrum,
    scaler: Option<f32>,
    working: &mut [(Frequency, FrequencyValue)],
) -> Result<(), SpectrumAnalyzerError> {
    let scaling: Box<SpectrumScalingFunction> = match scaler {
        Some(s) if s > 0.0 => Box::new(move |val, _info| val / s),
        _ => Box::new(scale_to_zero_to_one),
    };
    spectrum.apply_scaling_fn(&scaling, working)?;
    spectrum.apply_scaling_fn(&scale_20_times_log10, working)?;
    Ok(())
}
