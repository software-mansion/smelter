//! Stream-integrity primitives shared between the test harness and the
//! waveform inspector.
//!
//! The harness uses these to fail tests when the actual dump has new
//! gaps or discontinuities the expected dump does not. The inspector
//! uses the exact same primitives to draw the gap / artifact lanes,
//! which keeps "what the test sees" and "what the human sees after a
//! failure" perfectly aligned.

use std::time::Duration;

use crate::audio_decoder::AudioSampleBatch;

/// OPUS clock rate used everywhere in this crate.
pub const SAMPLE_RATE: u32 = 48_000;

/// Gaps shorter than this are ignored — they're well within the noise
/// of decoder timing jitter.
pub const GAP_THRESHOLD: Duration = Duration::from_micros(50);

/// Half-window (in samples) over which the artifact detector averages
/// `|d1|` to form the local baseline. ~1.3 ms at 48 kHz — small enough
/// that the baseline tracks fast dynamic changes.
pub const ARTIFACT_WINDOW_RADIUS: usize = 64;
/// Multiplier on the local mean of `|d1|` above which a sample is
/// flagged as a step.
pub const ARTIFACT_D1_MULT: f32 = 7.0;
/// Absolute floor (as fraction of global peak) below which a candidate
/// is ignored even if the relative threshold fires.
pub const ARTIFACT_D1_FLOOR_FRAC: f32 = 0.05;
/// Flagged samples within this many samples of each other are merged
/// into a single interval.
pub const ARTIFACT_MERGE_GAP: usize = 64;

/// Convert a presentation timestamp to a sample index on the
/// [`SAMPLE_RATE`] timeline.
pub fn pts_to_sample(pts: Duration) -> usize {
    (pts.as_secs_f64() * SAMPLE_RATE as f64) as usize
}

pub fn peak_abs(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max)
}

/// Demultiplex interleaved stereo chunks into two contiguous mono
/// buffers indexed by sample number — `[L, R]`. Each chunk's samples
/// are placed starting at `pts * SAMPLE_RATE`, so gaps in the input
/// become silence and reordered chunks still land at the right place.
pub fn chunks_to_stereo(chunks: &[AudioSampleBatch]) -> [Vec<f32>; 2] {
    if chunks.is_empty() {
        return [Vec::new(), Vec::new()];
    }
    let mut max_end_sample = 0_usize;
    for c in chunks {
        let start = pts_to_sample(c.pts);
        let end = start + c.samples.len() / 2;
        max_end_sample = max_end_sample.max(end);
    }
    let mut l = vec![0.0_f32; max_end_sample];
    let mut r = vec![0.0_f32; max_end_sample];
    for c in chunks {
        let start = pts_to_sample(c.pts);
        for (i, pair) in c.samples.chunks_exact(2).enumerate() {
            let idx = start + i;
            if idx < l.len() {
                l[idx] = pair[0];
                r[idx] = pair[1];
            }
        }
    }
    [l, r]
}

pub fn mix_to_mono(left: &[f32], right: &[f32]) -> Vec<f32> {
    let n = left.len().max(right.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let l = left.get(i).copied().unwrap_or(0.0);
        let r = right.get(i).copied().unwrap_or(0.0);
        out.push((l + r) * 0.5);
    }
    out
}

/// Walk the chunk list (assumed in pts order) and return the
/// `[start_sample, end_sample)` ranges where the next chunk starts at
/// least [`GAP_THRESHOLD`] after the previous one ended. Overlaps and
/// sub-threshold jitter are ignored.
pub fn compute_gaps(chunks: &[AudioSampleBatch]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for pair in chunks.windows(2) {
        let prev_dur =
            Duration::from_secs_f64((pair[0].samples.len() / 2) as f64 / SAMPLE_RATE as f64);
        let prev_end = pair[0].pts + prev_dur;
        let next_start = pair[1].pts;
        if next_start <= prev_end {
            continue;
        }
        if next_start - prev_end < GAP_THRESHOLD {
            continue;
        }
        out.push((pts_to_sample(prev_end), pts_to_sample(next_start)));
    }
    out
}

/// Run the discontinuity detector on each chunk independently (so
/// chunk-boundary zeros from `chunks_to_stereo` don't generate false
/// positives), once per channel, then merge all flagged intervals on
/// the global timeline.
pub fn detect_artifacts(chunks: &[AudioSampleBatch], peak: f32) -> Vec<(usize, usize)> {
    let mut all = Vec::new();
    let mut chunk_l: Vec<f32> = Vec::new();
    let mut chunk_r: Vec<f32> = Vec::new();
    for c in chunks {
        let frames = c.samples.len() / 2;
        chunk_l.clear();
        chunk_r.clear();
        chunk_l.reserve(frames);
        chunk_r.reserve(frames);
        for pair in c.samples.chunks_exact(2) {
            chunk_l.push(pair[0]);
            chunk_r.push(pair[1]);
        }
        let start = pts_to_sample(c.pts);
        for (s, e) in detect_artifacts_one(&chunk_l, peak) {
            all.push((start + s, start + e));
        }
        for (s, e) in detect_artifacts_one(&chunk_r, peak) {
            all.push((start + s, start + e));
        }
    }
    all.sort_by_key(|x| x.0);
    merge_overlapping(all)
}

/// Detector for a single contiguous mono buffer. Flags samples where
/// `|d1|` exceeds its sliding-window mean by a configured multiple
/// AND clears an absolute floor.
fn detect_artifacts_one(samples: &[f32], peak: f32) -> Vec<(usize, usize)> {
    let n = samples.len();
    if n < 2 {
        return Vec::new();
    }
    let mut d1 = vec![0.0_f32; n];
    for i in 1..n {
        d1[i] = (samples[i] - samples[i - 1]).abs();
    }
    let mean_d1 = sliding_mean(&d1, ARTIFACT_WINDOW_RADIUS);
    let floor_d1 = peak * ARTIFACT_D1_FLOOR_FRAC;
    let mut flagged = vec![false; n];
    for i in 1..n {
        if d1[i] > floor_d1 && d1[i] > ARTIFACT_D1_MULT * mean_d1[i].max(1.0) {
            flagged[i] = true;
        }
    }
    intervals_from_flags(&flagged, ARTIFACT_MERGE_GAP)
}

pub fn sliding_mean(values: &[f32], radius: usize) -> Vec<f32> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    let mut out = vec![0.0_f32; n];
    let mut sum = 0.0_f64;
    let mut count = 0usize;
    for v in values.iter().take(radius.min(n - 1) + 1) {
        sum += *v as f64;
        count += 1;
    }
    out[0] = (sum / count as f64) as f32;
    for i in 1..n {
        if i + radius < n {
            sum += values[i + radius] as f64;
            count += 1;
        }
        if let Some(rem) = i.checked_sub(radius + 1) {
            sum -= values[rem] as f64;
            count = count.saturating_sub(1);
        }
        out[i] = if count > 0 {
            (sum / count as f64) as f32
        } else {
            0.0
        };
    }
    out
}

/// Walk a `flagged` boolean array and produce contiguous intervals.
/// Adjacent flagged regions separated by fewer than `merge_gap`
/// non-flagged samples are coalesced into one interval.
pub fn intervals_from_flags(flagged: &[bool], merge_gap: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let n = flagged.len();
    let mut i = 0;
    while i < n {
        if !flagged[i] {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i + 1;
        i += 1;
        while i < n {
            if flagged[i] {
                end = i + 1;
                i += 1;
            } else {
                let bound = (i + merge_gap).min(n);
                if (i..bound).any(|k| flagged[k]) {
                    i += 1;
                } else {
                    break;
                }
            }
        }
        out.push((start, end));
    }
    out
}

pub fn merge_overlapping(intervals: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (s, e) in intervals {
        if let Some(last) = out.last_mut()
            && s <= last.1
        {
            last.1 = last.1.max(e);
            continue;
        }
        out.push((s, e));
    }
    out
}

/// Sum of total samples flagged by all `intervals` (length of all
/// `[start, end)` ranges).
pub fn total_flagged_samples(intervals: &[(usize, usize)]) -> usize {
    intervals.iter().map(|(s, e)| e.saturating_sub(*s)).sum()
}

/// Subtract `baseline` from `actual` on the sample-index axis: returns
/// the portions of `actual` intervals that are not covered by any
/// baseline interval. Used by validation to flag only the gaps /
/// artifacts that are *new* in the actual stream — anything already
/// present in the expected snapshot is intentionally tolerated.
pub fn subtract_intervals(
    actual: &[(usize, usize)],
    baseline: &[(usize, usize)],
    slack: usize,
) -> Vec<(usize, usize)> {
    let baseline = grow_intervals(baseline, slack);
    let mut out = Vec::new();
    for &(mut s, e) in actual {
        if e <= s {
            continue;
        }
        for &(bs, be) in &baseline {
            if be <= s || bs >= e {
                continue;
            }
            if bs <= s {
                s = s.max(be);
            } else {
                out.push((s, bs));
                s = be;
            }
            if s >= e {
                break;
            }
        }
        if s < e {
            out.push((s, e));
        }
    }
    out
}

fn grow_intervals(intervals: &[(usize, usize)], slack: usize) -> Vec<(usize, usize)> {
    let mut grown: Vec<(usize, usize)> = intervals
        .iter()
        .map(|(s, e)| (s.saturating_sub(slack), e.saturating_add(slack)))
        .collect();
    grown.sort_by_key(|i| i.0);
    merge_overlapping(grown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(pts_ms: u64, frames: usize) -> AudioSampleBatch {
        AudioSampleBatch {
            samples: vec![0.0; frames * 2],
            pts: Duration::from_millis(pts_ms),
        }
    }

    #[test]
    fn compute_gaps_ignores_back_to_back_chunks() {
        // 480 frames @ 48 kHz = 10 ms.
        let chunks = vec![batch(0, 480), batch(10, 480), batch(20, 480)];
        assert!(compute_gaps(&chunks).is_empty());
    }

    #[test]
    fn compute_gaps_flags_real_gap() {
        // Chunk0 ends at 10 ms, Chunk1 starts at 30 ms → 20 ms gap.
        let chunks = vec![batch(0, 480), batch(30, 480)];
        let gaps = compute_gaps(&chunks);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0], (480, 1440));
    }

    #[test]
    fn subtract_intervals_drops_known_gaps() {
        let actual = vec![(0, 100), (200, 300), (500, 600)];
        let baseline = vec![(0, 110), (190, 310)];
        // (500, 600) survives, the rest are within (slack-grown) baseline.
        assert_eq!(subtract_intervals(&actual, &baseline, 0), vec![(500, 600)]);
    }

    #[test]
    fn subtract_intervals_partial_overlap() {
        let actual = vec![(0, 1000)];
        let baseline = vec![(200, 400), (600, 800)];
        assert_eq!(
            subtract_intervals(&actual, &baseline, 0),
            vec![(0, 200), (400, 600), (800, 1000)]
        );
    }

    #[test]
    fn merge_overlapping_combines_adjacent() {
        assert_eq!(
            merge_overlapping(vec![(0, 10), (5, 20), (30, 40)]),
            vec![(0, 20), (30, 40)]
        );
    }
}
