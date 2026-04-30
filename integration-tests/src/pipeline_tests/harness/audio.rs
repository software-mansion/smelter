//! New audio-dump comparison logic for pipeline tests.
//!
//! Compares the OPUS audio payload of two RTP dumps by:
//!   1. Decoding both sides into PCM chunks (PTS preserved).
//!   2. Running the [`super::audio_analysis`] gap and artifact
//!      detectors on each side independently.
//!   3. Failing the test only when the actual stream introduces gaps
//!      or artifacts that the expected stream does not already have —
//!      anything baked into the snapshot is tolerated, so updating the
//!      snapshot intentionally adopts whatever quirks were previously
//!      flagged.
//!   4. Optionally running the FFT-based spectrum comparison from
//!      [`super::fft`] over caller-provided time ranges. We compare
//!      spectra rather than raw samples because OPUS round-trips
//!      routinely shift the actual stream by a handful of samples;
//!      a per-index MSE would explode on that even though the
//!      streams sound identical.
//!
//! The same primitives drive the waveform inspector that opens after a
//! failed run, so a failure here lights up the same lane in the GUI.

use anyhow::{Result, bail};
use bytes::Bytes;
use tracing::warn;

use crate::{
    audio_decoder::{AudioChannels, AudioDecoder, AudioSampleBatch},
    find_packets_for_payload_type,
    pipeline_tests::harness::{
        audio_analysis::{
            SAMPLE_RATE, compute_gaps, detect_artifacts, peak_abs, subtract_intervals,
            total_flagged_samples,
        },
        fft::{self, FftCompareConfig},
    },
    unmarshal_packets,
};

/// RTP payload type smelter uses for OPUS audio.
const AUDIO_PAYLOAD_TYPE: u8 = 97;

/// Caller-tunable thresholds for the new audio comparison.
///
/// Defaults are deliberately strict: zero new gaps and a small new
/// artifact budget to absorb sub-sample numerical wobble between
/// otherwise-identical decodes.
pub struct AudioCompareConfig {
    /// Maximum new-gap *samples* (relative to the expected stream) the
    /// actual stream is allowed to introduce. New gaps usually mean
    /// dropped or late packets in the encoder.
    pub max_new_gap_samples: usize,
    /// Maximum new-artifact *samples* the actual stream is allowed to
    /// introduce. Artifacts are sample-level discontinuities (clicks,
    /// glitches).
    pub max_new_artifact_samples: usize,
    /// When subtracting expected gaps/artifacts from the actual side,
    /// grow the expected intervals by this many samples on each side
    /// to absorb minor timing drift.
    pub interval_match_slack_samples: usize,
    /// Optional FFT-based spectrum comparison. Phase-invariant, so
    /// surviving the gap/artifact check is the floor and this is the
    /// "do they sound the same?" check on top. `None` skips it.
    pub fft: Option<FftCompareConfig>,
}

impl Default for AudioCompareConfig {
    fn default() -> Self {
        Self {
            max_new_gap_samples: 0,
            // ~10 ms of cumulative new flagged samples — comfortably
            // above pure encoder jitter, well below anything a human
            // would call a glitch.
            max_new_artifact_samples: (SAMPLE_RATE / 100) as usize,
            // ±1 ms of slack when matching expected to actual.
            interval_match_slack_samples: (SAMPLE_RATE / 1000) as usize,
            fft: None,
        }
    }
}

/// Decode `expected` and `actual` and run the comparison.
pub fn compare(expected: &Bytes, actual: &Bytes, config: AudioCompareConfig) -> Result<()> {
    let expected_chunks = decode(expected)?;
    let actual_chunks = decode(actual)?;
    compare_chunks(&expected_chunks, &actual_chunks, &config)
}

/// Public for callers that already have decoded chunks (the inspector,
/// the harness's own glue layer).
pub fn compare_chunks(
    expected: &[AudioSampleBatch],
    actual: &[AudioSampleBatch],
    config: &AudioCompareConfig,
) -> Result<()> {
    if actual.is_empty() {
        bail!("actual audio stream is empty");
    }

    let expected_gaps = compute_gaps(expected);
    let actual_gaps = compute_gaps(actual);
    let new_gaps = subtract_intervals(
        &actual_gaps,
        &expected_gaps,
        config.interval_match_slack_samples,
    );
    let new_gap_samples = total_flagged_samples(&new_gaps);
    if !new_gaps.is_empty() {
        warn!(
            count = new_gaps.len(),
            samples = new_gap_samples,
            "audio: actual stream has gaps not present in expected"
        );
        for (s, e) in &new_gaps {
            warn!(
                "  new gap: [{:.3}s, {:.3}s)",
                *s as f64 / SAMPLE_RATE as f64,
                *e as f64 / SAMPLE_RATE as f64
            );
        }
    }

    let expected_peak = chunks_peak(expected);
    let actual_peak = chunks_peak(actual);
    let global_peak = expected_peak.max(actual_peak).max(1.0);
    let expected_artifacts = detect_artifacts(expected, global_peak);
    let actual_artifacts = detect_artifacts(actual, global_peak);
    let new_artifacts = subtract_intervals(
        &actual_artifacts,
        &expected_artifacts,
        config.interval_match_slack_samples,
    );
    let new_artifact_samples = total_flagged_samples(&new_artifacts);
    if !new_artifacts.is_empty() {
        warn!(
            count = new_artifacts.len(),
            samples = new_artifact_samples,
            "audio: actual stream has artifacts not present in expected"
        );
    }

    if new_gap_samples > config.max_new_gap_samples {
        bail!(
            "audio: actual stream has {new_gap_samples} new gap samples \
             (allowed {})",
            config.max_new_gap_samples
        );
    }
    if new_artifact_samples > config.max_new_artifact_samples {
        bail!(
            "audio: actual stream has {new_artifact_samples} new artifact samples \
             (allowed {})",
            config.max_new_artifact_samples
        );
    }

    if let Some(fft_config) = &config.fft {
        fft::compare(expected, actual, fft_config)?;
    }

    Ok(())
}

fn chunks_peak(chunks: &[AudioSampleBatch]) -> f32 {
    chunks
        .iter()
        .map(|c| peak_abs(&c.samples))
        .fold(0.0_f32, f32::max)
}

fn decode(dump: &Bytes) -> Result<Vec<AudioSampleBatch>> {
    let packets = unmarshal_packets(dump)?;
    let packets = find_packets_for_payload_type(&packets, AUDIO_PAYLOAD_TYPE);
    let mut decoder = AudioDecoder::new(SAMPLE_RATE, AudioChannels::Stereo)?;
    for packet in packets {
        decoder.decode(packet)?;
    }
    Ok(decoder.take_samples())
}
