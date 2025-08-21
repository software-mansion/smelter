use anyhow::Result;
use bytes::Bytes;
use spectrum_analyzer::{Frequency, FrequencySpectrum, FrequencyValue};
use tracing::{error, trace};

use crate::{
    audio::{AudioAnalyzeTolerance, AudioValidationConfig, SamplingInterval},
    audio_decoder::AudioDecoder,
    find_packets_for_payload_type, unmarshal_packets,
    validation::audio::{
        calc_level, compare_timestamps,
        fft::{calc_fft, scale_fft_spectrum},
        find_samples, find_timestamps, split_samples, Channel,
    },
};

#[derive(Debug)]
pub struct AnalyzeResult {
    average_level: f32,
    median_level: f32,
    max_frequency: f32,
    max_frequency_level: f32,
    frequency_resolution: f32,
    general_level: f64,
}

impl AnalyzeResult {
    pub fn new(spectrum: FrequencySpectrum, general_level: f64) -> Self {
        Self {
            average_level: spectrum.average().val(),
            median_level: spectrum.median().val(),
            max_frequency: spectrum.max().0.val(),
            max_frequency_level: spectrum.max().1.val(),
            frequency_resolution: spectrum.frequency_resolution(),
            general_level,
        }
    }

    pub fn compare(
        actual: &Self,
        expected: &Self,
        tolerance: &AudioAnalyzeTolerance,
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

pub fn analyze_samples(
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

pub fn validate(
    expected: &Bytes,
    actual: &Bytes,
    test_config: AudioValidationConfig,
) -> Result<()> {
    let AudioValidationConfig {
        sampling_intervals: time_intervals,
        channels,
        sample_rate,
        samples_per_batch,
        allowed_failed_batches,
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
