use spectrum_analyzer::{
    error::SpectrumAnalyzerError,
    samples_fft_to_spectrum,
    scaling::{scale_20_times_log10, scale_to_zero_to_one, SpectrumScalingFunction},
    windows::hann_window,
    Frequency, FrequencyLimit, FrequencySpectrum, FrequencyValue,
};

pub fn calc_fft(
    samples: &[f32],
    sample_rate: u32,
) -> Result<FrequencySpectrum, SpectrumAnalyzerError> {
    let samples = hann_window(samples);
    samples_fft_to_spectrum(&samples, sample_rate, FrequencyLimit::All, None)
}
pub fn scale_fft_spectrum(
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
