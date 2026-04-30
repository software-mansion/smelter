//! New video-dump comparison logic for pipeline tests.
//!
//! Walks paired decoded frames from two RTP dumps using
//! [`RtpVideoDiffIter`] (PTS-aligned, lazy) and fails the test if too
//! many pairs disagree by more than the configured MSE inside any of
//! the validation intervals.
//!
//! The pairing strategy already accounts for differing framerates (see
//! `RtpVideoDiffIter` docs), so the harness can compare without
//! assuming both sides advance in lockstep.

use std::{ops::Range, time::Duration};

use anyhow::{Result, bail};
use bytes::Bytes;
use smelter_render::{Frame, FrameData, YuvPlanes};
use tracing::warn;

use crate::tools::rtp_video_diff_iter::{FramePair, RtpVideoDiffIter};

pub struct VideoCompareConfig {
    /// Time ranges (in PTS space) over which paired frames are
    /// inspected. Pairs whose later side falls outside every interval
    /// are ignored.
    pub validation_intervals: Vec<Range<Duration>>,
    /// Per-pair MSE threshold (over Y, U, V planes combined). Pairs
    /// over this fail.
    pub max_mse: f64,
    /// Maximum number of failed pairs across all intervals before the
    /// test errors. A "failed pair" is any pair whose MSE exceeded the
    /// threshold, or whose frames couldn't be compared (resolution
    /// mismatch, missing side, unsupported format).
    pub max_failed_pairs: usize,
    /// Maximum allowed difference (frames per second) between the two
    /// sides' average framerates.
    pub max_framerate_drift_fps: f32,
}

impl Default for VideoCompareConfig {
    fn default() -> Self {
        Self {
            validation_intervals: vec![Duration::from_secs(1)..Duration::from_secs(3)],
            max_mse: 20.0,
            max_failed_pairs: 0,
            max_framerate_drift_fps: 2.0,
        }
    }
}

/// Decode `expected` and `actual` lazily, pair by PTS, and check the
/// resulting pairs.
pub fn compare(expected: &Bytes, actual: &Bytes, config: VideoCompareConfig) -> Result<()> {
    let iter = RtpVideoDiffIter::from_bytes(expected, actual)?;

    let mut failed_pairs = 0usize;
    let mut total_pairs = 0usize;
    let mut last_expected: Option<Duration> = None;
    let mut first_expected: Option<Duration> = None;
    let mut expected_count: usize = 0;
    let mut last_actual: Option<Duration> = None;
    let mut first_actual: Option<Duration> = None;
    let mut actual_count: usize = 0;

    for pair in iter {
        let pair = pair?;
        track_pts(
            &pair.left,
            &mut first_expected,
            &mut last_expected,
            &mut expected_count,
        );
        track_pts(
            &pair.right,
            &mut first_actual,
            &mut last_actual,
            &mut actual_count,
        );

        if !pair_in_intervals(&pair, &config.validation_intervals) {
            continue;
        }
        total_pairs += 1;

        let result = check_pair(&pair, config.max_mse);
        if let Err(reason) = result {
            failed_pairs += 1;
            warn!(
                "video: pair @ expected={} actual={} failed: {reason}",
                format_pts(pair.left.as_ref()),
                format_pts(pair.right.as_ref())
            );
        }
    }

    if total_pairs == 0 {
        bail!(
            "video: no frame pairs fell inside the configured validation intervals \
             (expected_count={expected_count}, actual_count={actual_count})"
        );
    }

    let expected_fps = average_framerate(first_expected, last_expected, expected_count);
    let actual_fps = average_framerate(first_actual, last_actual, actual_count);
    if (expected_fps - actual_fps).abs() > config.max_framerate_drift_fps {
        bail!(
            "video: framerate mismatch (expected {expected_fps:.2} fps, actual {actual_fps:.2} fps, \
             allowed drift {:.2} fps)",
            config.max_framerate_drift_fps
        );
    }

    if failed_pairs > config.max_failed_pairs {
        bail!(
            "video: {failed_pairs} of {total_pairs} pairs failed (allowed {})",
            config.max_failed_pairs
        );
    }

    Ok(())
}

fn track_pts(
    side: &Option<Frame>,
    first: &mut Option<Duration>,
    last: &mut Option<Duration>,
    count: &mut usize,
) {
    if let Some(f) = side {
        // Frames advance the playhead; the iterator yields the same
        // side on consecutive steps until the other side's PTS catches
        // up, so we count *transitions* — a new last_pts means a new
        // frame on this side.
        if Some(f.pts) != *last {
            *count += 1;
            first.get_or_insert(f.pts);
        }
        *last = Some(f.pts);
    }
}

fn average_framerate(first: Option<Duration>, last: Option<Duration>, count: usize) -> f32 {
    if count <= 1 {
        return 0.0;
    }
    let (first, last) = match (first, last) {
        (Some(a), Some(b)) if b > a => (a, b),
        _ => return 0.0,
    };
    let secs = (last - first).as_secs_f32();
    if secs <= 0.0 {
        return 0.0;
    }
    (count - 1) as f32 / secs
}

fn pair_in_intervals(pair: &FramePair, intervals: &[Range<Duration>]) -> bool {
    let pts = match (pair.left.as_ref(), pair.right.as_ref()) {
        (Some(l), Some(r)) => l.pts.max(r.pts),
        (Some(f), None) | (None, Some(f)) => f.pts,
        (None, None) => return false,
    };
    intervals.iter().any(|range| range.contains(&pts))
}

fn check_pair(pair: &FramePair, max_mse: f64) -> Result<(), String> {
    let (Some(expected), Some(actual)) = (pair.left.as_ref(), pair.right.as_ref()) else {
        return Err("missing side (one stream ended early)".to_string());
    };
    if expected.resolution != actual.resolution {
        return Err(format!(
            "resolution mismatch ({:?} vs {:?})",
            expected.resolution, actual.resolution
        ));
    }
    let Some(mse) = pair_mse(expected, actual) else {
        return Err("incompatible frame formats".to_string());
    };
    if mse > max_mse {
        return Err(format!("mse={mse:.3} > {max_mse:.3}"));
    }
    Ok(())
}

/// Per-pixel MSE across Y, U, V planes. `None` when the frames have
/// formats the harness doesn't know how to compare.
pub(crate) fn pair_mse(expected: &Frame, actual: &Frame) -> Option<f64> {
    let (e, a) = match (&expected.data, &actual.data) {
        (FrameData::PlanarYuv420(e), FrameData::PlanarYuv420(a)) => (e, a),
        (FrameData::PlanarYuv422(e), FrameData::PlanarYuv422(a)) => (e, a),
        (FrameData::PlanarYuv444(e), FrameData::PlanarYuv444(a)) => (e, a),
        (FrameData::PlanarYuvJ420(e), FrameData::PlanarYuvJ420(a)) => (e, a),
        _ => return None,
    };
    let planes: [(&[u8], &[u8]); 3] = [
        (planes_y(e), planes_y(a)),
        (planes_u(e), planes_u(a)),
        (planes_v(e), planes_v(a)),
    ];
    let mut sum_sq: u64 = 0;
    let mut count: u64 = 0;
    for (lhs, rhs) in planes {
        if lhs.len() != rhs.len() {
            return None;
        }
        for (l, r) in lhs.iter().zip(rhs.iter()) {
            let d = i32::from(*l) - i32::from(*r);
            sum_sq += (d * d) as u64;
        }
        count += lhs.len() as u64;
    }
    if count == 0 {
        return None;
    }
    Some(sum_sq as f64 / count as f64)
}

fn planes_y(p: &YuvPlanes) -> &[u8] {
    &p.y_plane
}
fn planes_u(p: &YuvPlanes) -> &[u8] {
    &p.u_plane
}
fn planes_v(p: &YuvPlanes) -> &[u8] {
    &p.v_plane
}

fn format_pts(frame: Option<&Frame>) -> String {
    match frame {
        Some(f) => format!("{:.6}s", f.pts.as_secs_f64()),
        None => "—".to_string(),
    }
}
