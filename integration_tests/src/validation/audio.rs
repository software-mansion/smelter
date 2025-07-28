use anyhow::{Context, Result};
use bytes::Bytes;
use pitch_detection::detector::{mcleod::McLeodDetector, PitchDetector};
use spectrum_analyzer::{
    error::SpectrumAnalyzerError, samples_fft_to_spectrum, Frequency, FrequencyLimit,
    FrequencySpectrum, FrequencyValue,
};
use std::{ops::Range, time::Duration};

use crate::{
    audio_decoder::{AudioChannels, AudioDecoder, AudioSampleBatch, FFTAudioSampleBatch},
    find_packets_for_payload_type, unmarshal_packets,
};

struct FFTResult {
    average_magnitude: FrequencyValue,
    median_magnitude: FrequencyValue,
    magnitude_range: FrequencyValue,
    max_frequency: (Frequency, FrequencyValue),
    min_frequency: (Frequency, FrequencyValue),
}

// pub fn validate(
//     expected: &Bytes,
//     actual: &Bytes,
//     sampling_intervals: &[Range<Duration>],
//     allowed_error: f32,
//     channels: AudioChannels,
//     sample_rate: u32,
// ) -> Result<()> {
//     let expected_packets = unmarshal_packets(expected)?;
//     let actual_packets = unmarshal_packets(actual)?;
//     let expected_audio_packets = find_packets_for_payload_type(&expected_packets, 97);
//     let actual_audio_packets = find_packets_for_payload_type(&actual_packets, 97);
//
//     let mut expected_audio_decoder = AudioDecoder::new(sample_rate, channels)?;
//     let mut actual_audio_decoder = AudioDecoder::new(sample_rate, channels)?;
//
//     for packet in expected_audio_packets {
//         expected_audio_decoder.decode(packet)?;
//     }
//     for packet in actual_audio_packets {
//         actual_audio_decoder.decode(packet)?;
//     }
//
//     let expected_samples = expected_audio_decoder.take_samples();
//     let actual_samples = actual_audio_decoder.take_samples();
//
//     for time_range in sampling_intervals {
//         // let expected_batches = find_sample_batches(&expected_samples, time_range.clone());
//         // let actual_batches = find_sample_batches(&actual_samples, time_range.clone());
//         let expected_batches = find_sample_fft_batches(&expected_samples, time_range.clone());
//         let actual_batches = find_sample_fft_batches(&actual_samples, time_range.clone());
//
//         let (expected_pitch_left, expected_pitch_right) =
//             pitch_from_sample_batch(expected_batches, sample_rate)?;
//         let (actual_pitch_left, actual_pitch_right) =
//             pitch_from_sample_batch(actual_batches, sample_rate)?;
//
//         let diff_pitch_left = f64::abs(expected_pitch_left - actual_pitch_left);
//         let diff_pitch_right = f64::abs(expected_pitch_right - actual_pitch_right);
//
//         if diff_pitch_left > allowed_error as f64 || diff_pitch_right > allowed_error as f64 {
//             let pts_start = time_range.start.as_micros();
//             let pts_end = time_range.end.as_micros();
//
//             return Err(anyhow::anyhow!(
//                 "Audio mismatch. Time range: ({pts_start}, {pts_end}), Expected: ({}, {}) Actual: ({}, {})",
//                 expected_pitch_left, expected_pitch_right,
//                 actual_pitch_left, actual_pitch_right
//             ));
//         }
//     }
//     Ok(())
// }

pub fn validate(
    expected: &Bytes,
    actual: &Bytes,
    sampling_intervals: &[Range<Duration>],
    allowed_error: f32,
    channels: AudioChannels,
    sample_rate: u32,
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

    for time_range in sampling_intervals {
        let expected_batches = find_sample_fft_batches(&expected_samples, time_range.clone());
        let actual_batches = find_sample_fft_batches(&actual_samples, time_range.clone());

        let (expected_pitch_left, expected_pitch_right) =
            pitch_from_sample_batch(expected_batches, sample_rate)?;
        let (actual_pitch_left, actual_pitch_right) =
            pitch_from_sample_batch(actual_batches, sample_rate)?;

        let diff_pitch_left = f64::abs(expected_pitch_left - actual_pitch_left);
        let diff_pitch_right = f64::abs(expected_pitch_right - actual_pitch_right);

        if diff_pitch_left > allowed_error as f64 || diff_pitch_right > allowed_error as f64 {
            let pts_start = time_range.start.as_micros();
            let pts_end = time_range.end.as_micros();

            return Err(anyhow::anyhow!(
                "Audio mismatch. Time range: ({pts_start}, {pts_end}), Expected: ({}, {}) Actual: ({}, {})",
                expected_pitch_left, expected_pitch_right,
                actual_pitch_left, actual_pitch_right
            ));
        }
    }
    Ok(())
}

// fn find_sample_batches(
//     samples: &[AudioSampleBatch],
//     time_range: Range<Duration>,
// ) -> Vec<AudioSampleBatch> {
//     samples
//         .iter()
//         .filter(|s| time_range.contains(&s.pts))
//         .cloned()
//         .collect()
// }

fn find_sample_fft_batches(
    samples: &[AudioSampleBatch],
    time_range: Range<Duration>,
) -> Vec<FFTAudioSampleBatch> {
    samples
        .iter()
        .filter(|s| time_range.contains(&s.pts))
        .cloned()
        .map(|s| s.into())
        .collect()
}

// fn pitch_from_sample_batch(
//     sample_batch: Vec<AudioSampleBatch>,
//     sample_rate: u32,
// ) -> Result<(f64, f64)> {
//     fn get_pitch(samples: &[f64], sample_rate: u32) -> Result<f64> {
//         if samples.is_empty() {
//             return Err(anyhow::anyhow!("No samples"));
//         }
//         let mut detector: McLeodDetector<f64> = McLeodDetector::new(samples.len(), 0);
//         detector
//             .get_pitch(samples, sample_rate as usize, 0.0, 0.0)
//             .context("No pitch found")
//             .map(|pitch| pitch.frequency)
//     }
//
//     let left_samples = sample_batch
//         .iter()
//         .flat_map(|batch| &batch.samples)
//         .step_by(2)
//         .map(|sample| *sample as f64 / i16::MAX as f64)
//         .collect::<Vec<_>>();
//
//     let right_samples = sample_batch
//         .iter()
//         .flat_map(|batch| &batch.samples)
//         .skip(1)
//         .step_by(2)
//         .map(|sample| *sample as f64 / i16::MAX as f64)
//         .collect::<Vec<_>>();
//
//     Ok((
//         get_pitch(&left_samples, sample_rate)?,
//         get_pitch(&right_samples, sample_rate)?,
//     ))
// }

fn fft_result_from_sample_batch(
    sample_batch: Vec<FFTAudioSampleBatch>,
    sample_rate: u32,
) -> Result<(FFTResult, FFTResult)> {
    fn calc_fft(samples: &[f32], sample_rate: u32) -> Result<FFTResult, SpectrumAnalyzerError> {
        let spectrum = samples_fft_to_spectrum(samples, sample_rate, FrequencyLimit::All, None)?;

        Ok(FFTResult {
            average_magnitude: spectrum.average(),
            median_magnitude: spectrum.median(),
            magnitude_range: spectrum.range(),
            max_frequency: spectrum.max(),
            min_frequency: spectrum.min(),
        })
    }

    let samples_left = sample_batch
        .iter()
        .flat_map(|batch| &batch.samples)
        .step_by(2)
        .map(|s| *s)
        .collect::<Vec<_>>();

    let samples_right = sample_batch
        .iter()
        .flat_map(|batch| &batch.samples)
        .skip(1)
        .step_by(2)
        .map(|s| *s)
        .collect::<Vec<_>>();

    let fft_res_left = calc_fft(&samples_left, sample_rate)?;
    let fft_res_right = calc_fft(&samples_right, sample_rate)?;
    Ok((fft_res_left, fft_res_right))
}
