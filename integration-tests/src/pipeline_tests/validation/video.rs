use std::{ops::Range, time::Duration};

use anyhow::{Result, bail};
use smelter_render::{Frame, FrameData};
use tracing::warn;
use webrtc::rtp;

use crate::video_decoder::VideoDecoder;

use super::pair_by_pts;

pub struct VideoCompareConfig {
    /// Only frames whose expected PTS falls inside this range are compared.
    pub validation_range: Range<Duration>,
    /// Max PTS drift between a matched expected/actual frame. A frame missing
    /// outside this window counts as a missing frame.
    pub pts_tolerance: Duration,
    /// Per-plane MSE threshold. If any of Y/U/V exceeds this, the frame is
    /// counted as a content mismatch.
    pub allowed_plane_mse: f32,
    /// Number of missing-or-mismatched frames tolerated in the range.
    pub allowed_bad_frames: usize,
}

impl Default for VideoCompareConfig {
    fn default() -> Self {
        Self {
            validation_range: Duration::from_secs(1)..Duration::from_secs(3),
            pts_tolerance: Duration::from_millis(5),
            allowed_plane_mse: 20.0,
            allowed_bad_frames: 0,
        }
    }
}

pub fn compare(
    expected_packets: &[rtp::packet::Packet],
    actual_packets: &[rtp::packet::Packet],
    config: &VideoCompareConfig,
) -> Result<()> {
    let expected_frames = decode(expected_packets)?;
    let actual_frames = decode(actual_packets)?;

    let expected: Vec<&Frame> = expected_frames
        .iter()
        .filter(|f| config.validation_range.contains(&f.pts))
        .collect();
    let actual: Vec<&Frame> = actual_frames
        .iter()
        .filter(|f| {
            let widened = config.validation_range.start.saturating_sub(config.pts_tolerance)
                ..config.validation_range.end + config.pts_tolerance;
            widened.contains(&f.pts)
        })
        .collect();

    if expected.is_empty() {
        bail!(
            "no expected video frames inside validation range {:?}",
            config.validation_range
        );
    }

    let pairs = pair_by_pts(&expected, &actual, config.pts_tolerance, |f| f.pts);

    let mut bad = 0usize;
    let mut first_error: Option<String> = None;

    for (idx, (exp, paired)) in expected.iter().zip(pairs.iter()).enumerate() {
        let Some(j) = paired else {
            bad += 1;
            let msg = format!("frame #{idx} (pts={:?}) missing in actual", exp.pts);
            warn!("{msg}");
            first_error.get_or_insert(msg);
            continue;
        };
        let act = actual[*j];
        match mse_per_plane(exp, act) {
            Ok((y, u, v)) => {
                if y > config.allowed_plane_mse
                    || u > config.allowed_plane_mse
                    || v > config.allowed_plane_mse
                {
                    bad += 1;
                    let msg = format!(
                        "frame #{idx} (pts={:?}) content mismatch: mse_y={y:.2} mse_u={u:.2} mse_v={v:.2} (threshold {})",
                        exp.pts, config.allowed_plane_mse,
                    );
                    warn!("{msg}");
                    first_error.get_or_insert(msg);
                }
            }
            Err(e) => {
                bad += 1;
                let msg = format!("frame #{idx} (pts={:?}): {e}", exp.pts);
                warn!("{msg}");
                first_error.get_or_insert(msg);
            }
        }
    }

    if bad > config.allowed_bad_frames {
        bail!(
            "{bad} bad video frame(s) in range {:?} (allowed {}); first: {}",
            config.validation_range,
            config.allowed_bad_frames,
            first_error.unwrap_or_default(),
        );
    }

    Ok(())
}

fn decode(packets: &[rtp::packet::Packet]) -> Result<Vec<Frame>> {
    let mut decoder = VideoDecoder::new()?;
    for packet in packets {
        decoder.decode(packet.clone())?;
    }
    decoder.take_frames()
}

fn mse_per_plane(expected: &Frame, actual: &Frame) -> Result<(f32, f32, f32)> {
    let FrameData::PlanarYuv420(exp) = &expected.data else {
        bail!("unsupported expected frame format");
    };
    let FrameData::PlanarYuv420(act) = &actual.data else {
        bail!("unsupported actual frame format");
    };
    if expected.resolution != actual.resolution {
        bail!(
            "resolution mismatch: expected {:?}, actual {:?}",
            expected.resolution,
            actual.resolution
        );
    }
    Ok((
        mse(&exp.y_plane, &act.y_plane),
        mse(&exp.u_plane, &act.u_plane),
        mse(&exp.v_plane, &act.v_plane),
    ))
}

fn mse(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }
    let sq: f32 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as i32 - *y as i32).pow(2) as f32)
        .sum();
    sq / a.len() as f32
}
