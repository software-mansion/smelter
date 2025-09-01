use std::{collections::HashMap, time::Duration};

use compositor_render::InputId;
use tracing::warn;

use super::InputSamplesSet;
use crate::prelude::*;

#[cfg(test)]
mod consecutive_frames_tests;
#[cfg(test)]
mod single_frame_tests;

pub(super) fn expected_samples_count(start: Duration, end: Duration, sample_rate: u32) -> usize {
    (end.saturating_sub(start).as_nanos() * sample_rate as u128 / 1_000_000_000) as usize
}
pub(super) fn prepare_input_samples(
    input_samples_set: InputSamplesSet,
    mixing_sample_rate: u32,
) -> HashMap<InputId, Vec<(f64, f64)>> {
    input_samples_set
        .samples
        .into_iter()
        .map(|(input_id, input_batch)| {
            let samples = frame_input_samples(
                input_samples_set.start_pts,
                input_samples_set.end_pts,
                input_batch,
                mixing_sample_rate,
            );

            (input_id, samples)
        })
        .collect()
}

/// Produce continuous batch of samples for range (start_pts, end_pts).
///
/// This code assumes that start_pts and end_pts are always numerically correct. Code that
/// generates those timestamps needs to ensure that.
///
/// How to define pts of a single sample in batch:
/// - Sample has a start time, the first item in a sample batch starts at the same time as batch PTS.
/// - Sample has an end time, the first item in a sample batch ends `1/sample_rate` seconds later.
/// - Each consecutive sample in the batch is starting when the previous one has ended.
/// - Input and output samples are out of sync, so all samples need to be shifted to match.
///
/// For the sample to be included in the output range:
/// - start_pts of a sample >= start_pts of an output batch (after applying `sample_offset`).
/// - end_pts of a sample <= end_pts of an output batch (after applying `sample_offset`).
/// - `=` in above cases means close enough to be a precision related error.
fn frame_input_samples(
    start_pts: Duration,
    end_pts: Duration,
    samples: Vec<InputAudioSamples>,
    sample_rate: u32,
) -> Vec<(f64, f64)> {
    let mut samples_in_frame = Vec::new();

    // Real numerical errors are a lot smaller, but taking max error as 1% of a sample duration
    // seems to be safe enough.
    let max_error = Duration::from_secs_f64(0.01 / sample_rate as f64);

    // Output and input samples have the same sample rate, but they are not synced with each
    // other. We need to calculate an offset between input and output samples. This value
    // should be constant for a specific input, but there is no harm with calculating for every
    // frame.
    let sample_offset = samples
        .first()
        .map(|batch| {
            let duration_secs = start_pts.as_secs_f64() - batch.start_pts.as_secs_f64();
            let sample_duration_secs = 1.0 / sample_rate as f64;
            let offset_secs = duration_secs.rem_euclid(sample_duration_secs);
            println!(
                "Calculated offser: {:?}",
                Duration::from_secs_f64(offset_secs),
            );

            // This is necessary because of numerical inconsistence in calculation of
            // `duration_secs`. If offset is set to any value divisible by sample length then an
            // underflow may occur that sets the offset to the full sample length.
            if offset_secs > 0.997 * sample_duration_secs {
                Duration::ZERO
            } else {
                Duration::from_secs_f64(offset_secs)
            }
        })
        .unwrap_or(Duration::ZERO);
    println!("Offset: {sample_offset:?}");

    let time_to_sample_count = |duration: Duration| {
        let sample_count = duration.as_secs_f64() * sample_rate as f64;
        // If value is close to the integer then round it, otherwise fallback to standard
        // integer division behavior. Close is defined as 1% of a sample (the same as max_error).
        if (sample_count - sample_count.round()).abs() < 0.01 {
            sample_count.round() as usize
        } else {
            sample_count.floor() as usize
        }
    };

    let last_batch_end_pts = samples.last().map(|sample| sample.end_pts + sample_offset);
    let samples_iter = samples.into_iter().map(|mut sample| {
        sample.start_pts += sample_offset;
        sample.end_pts += sample_offset;
        sample
    });

    for input_samples in samples_iter {
        let sample_count = samples_in_frame.len();
        let expected_next_sample_start_pts =
            start_pts + Duration::from_secs_f64(sample_count as f64 / sample_rate as f64);

        // Potentially fill missing spots
        if expected_next_sample_start_pts + max_error < input_samples.start_pts {
            let missing_time = input_samples
                .start_pts
                .saturating_sub(expected_next_sample_start_pts);
            let missing_samples_count = time_to_sample_count(missing_time);

            // NOTE: This needs reviewing as it does not make sense at first glance.
            if missing_samples_count < 1 {
                warn!(
                    ?missing_time,
                    "Distance between samples is higher than expected."
                )
            }
            samples_in_frame.extend((0..missing_samples_count).map(|_| (0f64, 0f64)))
        }

        let sample_count = samples_in_frame.len();
        let expected_next_sample_start_pts =
            start_pts + Duration::from_secs_f64(sample_count as f64 / sample_rate as f64);

        // Check if we need to drop samples at the beginning
        let mut start_range = 0;
        if expected_next_sample_start_pts > input_samples.start_pts + max_error {
            let time_to_remove_from_start =
                expected_next_sample_start_pts.saturating_sub(input_samples.start_pts);
            let samples_to_remove_from_start = time_to_sample_count(time_to_remove_from_start);
            if sample_count != 0 {
                // We should only drop samples in the first batch that overlaps with target batch
                // timestamps.
                warn!(
                    "Received overlapping batches on input. Dropping {samples_to_remove_from_start} samples.",
                );
            }
            start_range = usize::min(samples_to_remove_from_start, input_samples.samples.len());
        };

        // Check if we need to drop samples at the end
        let mut end_range = input_samples.len();
        if input_samples.end_pts > end_pts + max_error {
            let desired_duration = end_pts.saturating_sub(expected_next_sample_start_pts);
            let desired_sample_count = time_to_sample_count(desired_duration);
            end_range = usize::min(
                start_range + desired_sample_count,
                input_samples.samples.len(),
            );
        }

        samples_in_frame.extend(input_samples.samples[start_range..end_range].iter());
    }

    // Fill at the end only if last batch is ending to quickly
    if last_batch_end_pts.unwrap_or(start_pts) < end_pts + max_error {
        ensure_correct_amount_of_samples(start_pts, end_pts, sample_rate, &mut samples_in_frame);
    }

    check_frame_samples(start_pts, end_pts, sample_rate, &samples_in_frame);

    // This call ensures that input buffer has correct amount of samples,
    // but if it needs to do anything it is considered a bug.
    ensure_correct_amount_of_samples(start_pts, end_pts, sample_rate, &mut samples_in_frame);

    samples_in_frame
}

fn check_frame_samples(
    start_pts: Duration,
    end_pts: Duration,
    sample_rate: u32,
    samples: &[(f64, f64)],
) {
    let samples_count_times_1e9 =
        end_pts.saturating_sub(start_pts).as_nanos() * sample_rate as u128;
    if samples_count_times_1e9 % 1_000_000_000 != 0 {
        warn!(
            "Duration {:?} is not divisible by sample duration (sample rate: {}).",
            end_pts.saturating_sub(start_pts),
            sample_rate,
        )
    }
    if samples.len() as u128 != samples_count_times_1e9 / 1_000_000_000 {
        warn!(
            "Wrong amount of samples generated. Expected: {}, Actual: {}.",
            samples_count_times_1e9 / 1_000_000_000,
            samples.len()
        );
    }
}

fn ensure_correct_amount_of_samples(
    start: Duration,
    end: Duration,
    sample_rate: u32,
    samples_buffer: &mut Vec<(f64, f64)>,
) {
    // This is precise as long as (end - start) is divisible by `1/sample_rate`
    let expected_samples_count = expected_samples_count(start, end, sample_rate);
    if expected_samples_count > samples_buffer.len() {
        let missing_samples_count = expected_samples_count - samples_buffer.len();
        let missing_samples = (0..missing_samples_count).map(|_| (0f64, 0f64));
        samples_buffer.extend(missing_samples);
    } else {
        samples_buffer.drain(expected_samples_count..samples_buffer.len());
    }
}
