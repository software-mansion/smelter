use std::time::Duration;

use anyhow::{Result, bail};
use tracing::warn;
use webrtc::rtp;

use crate::audio_decoder::{AudioChannels, AudioDecoder, AudioSampleBatch};

use super::pair_by_pts;

pub struct AudioCompareConfig {
    pub sample_rate: u32,
    pub channels: AudioChannels,
    /// Max PTS drift between a matched expected/actual batch.
    pub pts_tolerance: Duration,
    /// Allowed per-sample RMS difference between paired batches. Samples are
    /// f32 in [-1.0, 1.0] so an RMS diff of 0.05 is ~-26 dBFS of noise.
    pub allowed_rms_diff: f32,
    /// Number of missing or mismatched batches tolerated.
    pub allowed_bad_batches: usize,
}

impl Default for AudioCompareConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: AudioChannels::Stereo,
            pts_tolerance: Duration::from_millis(10),
            allowed_rms_diff: 0.05,
            allowed_bad_batches: 0,
        }
    }
}

pub fn compare(
    expected_packets: &[rtp::packet::Packet],
    actual_packets: &[rtp::packet::Packet],
    config: &AudioCompareConfig,
) -> Result<()> {
    let expected = decode(expected_packets, config)?;
    let actual = decode(actual_packets, config)?;

    if expected.is_empty() {
        bail!("no expected audio batches produced");
    }

    let pairs = pair_by_pts(&expected, &actual, config.pts_tolerance, |b| b.pts);

    let mut bad = 0usize;
    let mut first_error: Option<String> = None;

    for (idx, (exp, paired)) in expected.iter().zip(pairs.iter()).enumerate() {
        let Some(j) = paired else {
            bad += 1;
            let msg = format!("batch #{idx} (pts={:?}) missing in actual", exp.pts);
            warn!("{msg}");
            first_error.get_or_insert(msg);
            continue;
        };
        let act = &actual[*j];
        match rms_diff(&exp.samples, &act.samples) {
            Ok(diff) if diff > config.allowed_rms_diff => {
                bad += 1;
                let msg = format!(
                    "batch #{idx} (pts={:?}) content mismatch: rms_diff={diff:.4} (threshold {})",
                    exp.pts, config.allowed_rms_diff,
                );
                warn!("{msg}");
                first_error.get_or_insert(msg);
            }
            Ok(_) => {}
            Err(e) => {
                bad += 1;
                let msg = format!("batch #{idx} (pts={:?}): {e}", exp.pts);
                warn!("{msg}");
                first_error.get_or_insert(msg);
            }
        }
    }

    if bad > config.allowed_bad_batches {
        bail!(
            "{bad} bad audio batch(es) (allowed {}); first: {}",
            config.allowed_bad_batches,
            first_error.unwrap_or_default(),
        );
    }

    Ok(())
}

fn decode(
    packets: &[rtp::packet::Packet],
    config: &AudioCompareConfig,
) -> Result<Vec<AudioSampleBatch>> {
    let mut decoder = AudioDecoder::new(config.sample_rate, config.channels)?;
    for packet in packets {
        decoder.decode(packet.clone())?;
    }
    Ok(decoder.take_samples())
}

fn rms_diff(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        bail!("sample count mismatch ({} vs {})", a.len(), b.len());
    }
    if a.is_empty() {
        return Ok(0.0);
    }
    let sq: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum();
    Ok((sq / a.len() as f32).sqrt())
}
