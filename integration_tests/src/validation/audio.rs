use anyhow::Result;
use bytes::Bytes;
use spectrum_analyzer::{
    error::SpectrumAnalyzerError, samples_fft_to_spectrum, scaling::scale_to_zero_to_one,
    windows::hann_window, Frequency, FrequencyLimit, FrequencyValue,
};
use std::{ops::Range, time::Duration};

use crate::{
    audio_decoder::{AudioChannels, AudioDecoder},
    find_packets_for_payload_type, unmarshal_packets,
    validation::FFTTolerance,
    SamplingInterval,
};

#[derive(Debug)]
struct FFTResult {
    average_magnitude: FrequencyValue,
    median_magnitude: FrequencyValue,
    max_frequency: (Frequency, FrequencyValue),
    avg_level: f64,
}

impl FFTResult {
    // TODO: @jbrs: Add tolerance
    fn compare(&self, other: &FFTResult) -> Result<()> {
        let values_match = self.average_magnitude == other.average_magnitude
            && self.median_magnitude == other.median_magnitude
            && self.max_frequency.0 == other.max_frequency.0
            && self.max_frequency.1 == other.max_frequency.1
            && self.avg_level == other.avg_level;

        if values_match {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Audio mismatch!: {:#?}, {:#?}",
                self,
                other
            ))
        }
    }
}

pub fn validate(
    expected: &Bytes,
    actual: &Bytes,
    time_intervals: &[Range<Duration>],

    // At current time it is set to stereo for all tests
    channels: AudioChannels,
    sample_rate: u32,
    samples_per_batch: usize,
    // TODO: @jbrs: Temporary
    _tolerance: FFTTolerance,
) -> Result<()> {
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

    let expected_samples = expected_audio_decoder.take_samples();
    let actual_samples = actual_audio_decoder.take_samples();

    let sampling_intervals = time_intervals
        .iter()
        .flat_map(|range| SamplingInterval::from_range(range, sample_rate, samples_per_batch))
        .collect::<Vec<_>>();

    for interval in sampling_intervals {
        let expected_samples = find_fft_samples(&expected_samples, interval);
        let actual_samples = find_fft_samples(&actual_samples, interval);

        let (expected_fft_left, expected_fft_right) =
            fft_result_from_samples(expected_samples, sample_rate)?;
        let (actual_fft_left, actual_fft_right) =
            fft_result_from_samples(actual_samples, sample_rate)?;

        actual_fft_left.compare(&expected_fft_left)?;
        actual_fft_right.compare(&expected_fft_right)?;
    }
    Ok(())
}

fn find_fft_samples(samples: &[f32], interval: SamplingInterval) -> Vec<f32> {
    let first_sample = interval.first_sample;
    let last_sample = interval.first_sample + interval.samples;
    samples[first_sample..last_sample].to_vec()
}

fn fft_result_from_samples(
    sample_batch: Vec<f32>,
    sample_rate: u32,
) -> Result<(FFTResult, FFTResult)> {
    fn calc_fft(samples: &[f32], sample_rate: u32) -> Result<FFTResult, SpectrumAnalyzerError> {
        let spectrum = samples_fft_to_spectrum(
            &hann_window(samples),
            sample_rate,
            FrequencyLimit::All,
            Some(&scale_to_zero_to_one),
        )?;
        let avg_level = (spectrum.average().val() as f64).log10() * 20.0;

        Ok(FFTResult {
            average_magnitude: spectrum.average(),
            median_magnitude: spectrum.median(),
            max_frequency: spectrum.max(),
            avg_level,
        })
    }

    let samples_left = sample_batch.iter().step_by(2).copied().collect::<Vec<_>>();

    let samples_right = sample_batch
        .iter()
        .skip(1)
        .step_by(2)
        .copied()
        .collect::<Vec<_>>();

    let fft_res_left = calc_fft(&samples_left, sample_rate)?;
    let fft_res_right = calc_fft(&samples_right, sample_rate)?;
    Ok((fft_res_left, fft_res_right))
}
