use anyhow::Result;
use bytes::Bytes;
use core::panic;
use smelter_render::{Frame, FrameData};
use std::{ops::Range, time::Duration};
use tracing::warn;

use crate::{find_packets_for_payload_type, unmarshal_packets, video_decoder::VideoDecoder};

pub struct VideoValidationConfig {
    pub validation_intervals: Vec<Range<Duration>>,
    pub allowed_error: f32,
    pub allowed_invalid_frames: usize,
}

impl Default for VideoValidationConfig {
    fn default() -> Self {
        Self {
            validation_intervals: vec![Duration::from_secs(1)..Duration::from_secs(3)],
            allowed_error: 20.0,
            allowed_invalid_frames: 0,
        }
    }
}

pub fn validate(
    expected: &Bytes,
    actual: &Bytes,
    validation_intervals: &[Range<Duration>],
    allowed_error: f32,
    allowed_invalid_frames: usize,
) -> Result<()> {
    let expected_packets = unmarshal_packets(expected)?;
    let actual_packets = unmarshal_packets(actual)?;

    let expected_packets = find_packets_for_payload_type(&expected_packets, 96);
    let actual_packets = find_packets_for_payload_type(&actual_packets, 96);

    let mut expected_video_decoder = VideoDecoder::new()?;
    let mut actual_video_decoder = VideoDecoder::new()?;

    for packet in expected_packets {
        expected_video_decoder.decode(packet)?;
    }
    for packet in actual_packets {
        actual_video_decoder.decode(packet)?;
    }

    let expected_frames = expected_video_decoder.take_frames()?;
    let actual_frames = actual_video_decoder.take_frames()?;

    for time_range in validation_intervals {
        let expected_frames = find_frames_for_time_range(&expected_frames, time_range);
        let actual_frames = find_frames_for_time_range(&actual_frames, time_range);

        let expected_framerate = average_framerate(&expected_frames);
        let actual_framerate = average_framerate(&actual_frames);

        if (expected_framerate - actual_framerate).abs() > 2.0 {
            return Err(anyhow::anyhow!(
                "Framerate mismatch. Expected: {expected_framerate}, Actual: {actual_framerate}"
            ));
        }

        let mut incorrect_frames_count =
            usize::abs_diff(expected_frames.len(), actual_frames.len());
        if incorrect_frames_count != 0 {
            warn!(
                ?time_range,
                expected = expected_frames.len(),
                actual = actual_frames.len(),
                "Frame count mismatch."
            )
        }
        for (i, (expected, actual)) in expected_frames.iter().zip(actual_frames.iter()).enumerate()
        {
            if let Err(err) = validate_frame(expected, actual, allowed_error) {
                warn!(?time_range, "Frame {i} mismatch. Error: {err}");
                incorrect_frames_count += 1;
            }
        }

        if incorrect_frames_count > allowed_invalid_frames {
            return Err(anyhow::anyhow!(
                "Too many incorrect frames: {} out of {} were incorrect.",
                incorrect_frames_count,
                expected_frames.len()
            ));
        }
    }

    Ok(())
}

fn average_framerate(frames: &[Frame]) -> f32 {
    if frames.len() <= 1 {
        return 0.0;
    }

    let mut total_duration = Duration::from_secs(0);
    for i in 1..frames.len() {
        let duration = frames[i].pts - frames[i - 1].pts;
        total_duration += duration;
    }

    let total_duration_secs = total_duration.as_secs_f32();
    (frames.len() - 1) as f32 / total_duration_secs
}

fn validate_frame(expected: &Frame, actual: &Frame, allowed_error: f32) -> Result<()> {
    let FrameData::PlanarYuv420(ref expected) = expected.data else {
        panic!("Invalid format");
    };
    let FrameData::PlanarYuv420(ref actual) = actual.data else {
        panic!("Invalid format");
    };
    let diff_y = calculate_mse(&expected.y_plane, &actual.y_plane);
    let diff_u = calculate_mse(&expected.u_plane, &actual.u_plane);
    let diff_v = calculate_mse(&expected.v_plane, &actual.v_plane);

    if diff_y > allowed_error || diff_u > allowed_error || diff_v > allowed_error {
        return Err(anyhow::anyhow!(
            "Diff Y: {diff_y}, Diff U: {diff_u}, Diff V: {diff_v}"
        ));
    }
    Ok(())
}

fn calculate_mse(expected: &[u8], actual: &[u8]) -> f32 {
    if expected.len() != actual.len() {
        return f32::MAX;
    }

    let square_error: f32 = expected
        .iter()
        .zip(actual.iter())
        .map(|(e, a)| (*e as i32 - *a as i32).pow(2) as f32)
        .sum();

    square_error / expected.len() as f32
}

fn find_frames_for_time_range(frames: &[Frame], pts: &Range<Duration>) -> Vec<Frame> {
    frames
        .iter()
        .filter(|f| pts.contains(&f.pts))
        .cloned()
        .collect()
}
