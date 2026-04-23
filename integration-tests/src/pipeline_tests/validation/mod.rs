//! PTS-aligned content comparison of two RTP dumps.
//!
//! Replaces the older `crate::validation` approach, which zipped decoded
//! frames/batches by index and only looked at aggregate framerate in a time
//! window. That hid per-frame timing regressions and propagated index drift
//! after a single dropped frame. Here each expected frame is paired with the
//! actual frame whose PTS is closest, within a small tolerance — so a missing
//! or mis-timed frame is detected locally instead of being averaged away.
//!
//! # PTS reference frame
//!
//! The audio/video decoders normalize RTP timestamps to the **first packet of
//! each stream** (PTS 0 = first packet seen). RTP start timestamps are random
//! per run, so this is what lets the two dumps be compared at all. The
//! implicit assumption is that both dumps start at the same logical moment —
//! a regression that drops only the leading N frames will shift the whole
//! timeline by N and surface as many MSE mismatches rather than a clean "N
//! leading frames missing" report. Accepted trade-off.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use bytes::Bytes;

use crate::{find_packets_for_payload_type, unmarshal_packets};

pub mod audio;
pub mod video;

pub const VIDEO_PAYLOAD_TYPE: u8 = 96;
pub const AUDIO_PAYLOAD_TYPE: u8 = 97;

pub struct CompareConfig {
    pub video: Option<video::VideoCompareConfig>,
    pub audio: Option<audio::AudioCompareConfig>,
}

pub fn compare_rtp_dumps(expected: &Bytes, actual: &Bytes, config: CompareConfig) -> Result<()> {
    let expected_packets = unmarshal_packets(expected).context("parsing expected dump")?;
    let actual_packets = unmarshal_packets(actual).context("parsing actual dump")?;

    let expected_has_video = expected_packets
        .iter()
        .any(|p| p.header.payload_type == VIDEO_PAYLOAD_TYPE);
    let actual_has_video = actual_packets
        .iter()
        .any(|p| p.header.payload_type == VIDEO_PAYLOAD_TYPE);
    let expected_has_audio = expected_packets
        .iter()
        .any(|p| p.header.payload_type == AUDIO_PAYLOAD_TYPE);
    let actual_has_audio = actual_packets
        .iter()
        .any(|p| p.header.payload_type == AUDIO_PAYLOAD_TYPE);

    if expected_has_video != actual_has_video {
        bail!(
            "video stream presence differs (expected {expected_has_video}, actual {actual_has_video})"
        );
    }
    if expected_has_audio != actual_has_audio {
        bail!(
            "audio stream presence differs (expected {expected_has_audio}, actual {actual_has_audio})"
        );
    }

    if let Some(video_config) = config.video {
        if !expected_has_video {
            bail!("video comparison requested but no video packets present in expected dump");
        }
        let expected_video = find_packets_for_payload_type(&expected_packets, VIDEO_PAYLOAD_TYPE);
        let actual_video = find_packets_for_payload_type(&actual_packets, VIDEO_PAYLOAD_TYPE);
        video::compare(&expected_video, &actual_video, &video_config)
            .context("video comparison")?;
    }

    if let Some(audio_config) = config.audio {
        if !expected_has_audio {
            bail!("audio comparison requested but no audio packets present in expected dump");
        }
        let expected_audio = find_packets_for_payload_type(&expected_packets, AUDIO_PAYLOAD_TYPE);
        let actual_audio = find_packets_for_payload_type(&actual_packets, AUDIO_PAYLOAD_TYPE);
        audio::compare(&expected_audio, &actual_audio, &audio_config)
            .context("audio comparison")?;
    }

    Ok(())
}

/// For each item in `expected` (keyed by PTS), find the index of the item in
/// `actual` with the closest PTS. If the closest item is further than
/// `tolerance` from the expected PTS, return `None` for that position. Each
/// actual item can only be matched to one expected item — if the best
/// candidate is already taken, the next closest unused one is used.
///
/// Returned vector has the same length as `expected`. The value at index `i`
/// is `Some(j)` when `actual[j]` is paired with `expected[i]`, `None` when
/// no unused actual item lies within tolerance.
pub(crate) fn pair_by_pts<T, F>(
    expected: &[T],
    actual: &[T],
    tolerance: Duration,
    pts_of: F,
) -> Vec<Option<usize>>
where
    F: Fn(&T) -> Duration,
{
    let mut used = vec![false; actual.len()];
    let mut pairs = Vec::with_capacity(expected.len());

    for exp in expected {
        let exp_pts = pts_of(exp);
        let mut best: Option<(usize, Duration)> = None;
        for (j, act) in actual.iter().enumerate() {
            if used[j] {
                continue;
            }
            let act_pts = pts_of(act);
            let diff = if act_pts >= exp_pts {
                act_pts - exp_pts
            } else {
                exp_pts - act_pts
            };
            if diff > tolerance {
                continue;
            }
            if best.is_none_or(|(_, d)| diff < d) {
                best = Some((j, diff));
            }
        }
        if let Some((j, _)) = best {
            used[j] = true;
            pairs.push(Some(j));
        } else {
            pairs.push(None);
        }
    }
    pairs
}

#[cfg(test)]
mod pair_tests {
    use super::*;

    fn d(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn closest_match_wins() {
        let exp = [d(100), d(200), d(300)];
        let act = [d(105), d(210), d(295)];
        let pairs = pair_by_pts(&exp, &act, d(20), |x| *x);
        assert_eq!(pairs, vec![Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn missing_frame_reported() {
        let exp = [d(100), d(200), d(300)];
        let act = [d(100), d(300)];
        let pairs = pair_by_pts(&exp, &act, d(20), |x| *x);
        assert_eq!(pairs, vec![Some(0), None, Some(1)]);
    }

    #[test]
    fn out_of_tolerance_is_none() {
        let exp = [d(100)];
        let act = [d(200)];
        let pairs = pair_by_pts(&exp, &act, d(20), |x| *x);
        assert_eq!(pairs, vec![None]);
    }

    #[test]
    fn each_actual_matched_once() {
        let exp = [d(100), d(101)];
        let act = [d(100)];
        let pairs = pair_by_pts(&exp, &act, d(20), |x| *x);
        assert_eq!(pairs, vec![Some(0), None]);
    }
}
