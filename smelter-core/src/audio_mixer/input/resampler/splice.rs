use std::{f64::consts::PI, time::Duration};

use audioadapter::Adapter;
use tracing::error;

use crate::{AudioSamples, utils::AudioSamplesBuffer};

/// Length of the waveform compared when searching for the best cut point. Covers a pitch period
/// down to 100Hz.
const CORRELATION_DURATION: Duration = Duration::from_millis(10);

/// How far the search may move the cut point from the requested one.
const SEARCH_DURATION: Duration = Duration::from_millis(5);

/// Length of the crossfade joining both sides of the cut. Has to be shorter than
/// `CORRELATION_DURATION`.
const CROSSFADE_DURATION: Duration = Duration::from_millis(5);

/// Remove about `frames` frames from the front of `buffer`. The cut point is moved by up to
/// `SEARCH_DURATION` to where the waveform best matches the front of `buffer`, and both sides of
/// the cut are joined with a crossfade.
///
/// Returns the number of removed frames, or `None` (leaving `buffer` untouched) if `buffer` is
/// too short to search for the cut point.
pub(super) fn drop_frames(
    buffer: &mut AudioSamplesBuffer,
    frames: usize,
    sample_rate: u32,
) -> Option<usize> {
    let window = (CORRELATION_DURATION.as_secs_f64() * sample_rate as f64).round() as usize;
    let search = (SEARCH_DURATION.as_secs_f64() * sample_rate as f64).round() as usize;
    let crossfade = (CROSSFADE_DURATION.as_secs_f64() * sample_rate as f64).round() as usize;

    // Cut can't overlap the reference window at the front.
    let search_start = usize::max(frames.saturating_sub(search), window);
    let search_end = usize::max(frames + search, search_start);
    if buffer.frames() < search_end + window {
        return None;
    }

    let reference = mid_samples(buffer, 0, window);
    let region = mid_samples(buffer, search_start, search_end - search_start + window);
    // Normalized cross-correlation, energy of `reference` is skipped because it is the same for
    // every candidate.
    let (best_offset, _) = (0..=search_end - search_start)
        .map(|offset| {
            let candidate = &region[offset..offset + window];
            let dot: f64 = reference.iter().zip(candidate).map(|(a, b)| a * b).sum();
            let energy: f64 = candidate.iter().map(|b| b * b).sum();
            let correlation = if energy > 0.0 {
                dot / energy.sqrt()
            } else {
                0.0
            };
            (offset, correlation)
        })
        .fold((0, f64::MIN), |best, candidate| {
            if candidate.1 > best.1 {
                candidate
            } else {
                best
            }
        });
    let cut = search_start + best_offset;

    let front = buffer.read_samples(crossfade);
    buffer.drain_samples(cut - crossfade);
    let target = buffer.read_samples(crossfade);
    buffer.push_front(crossfade_samples(front, target));
    Some(cut)
}

/// Fade out the front `CROSSFADE_DURATION` of `buffer` and drop everything after it.
pub(super) fn fade_out(buffer: &mut AudioSamplesBuffer, sample_rate: u32) {
    let crossfade = (CROSSFADE_DURATION.as_secs_f64() * sample_rate as f64).round() as usize;
    let samples = buffer.read_samples(usize::min(crossfade, buffer.frames()));
    buffer.clear();

    let len = samples.len();
    let fade_out = |i: usize| 1.0 - fade_in_gain(i, len);
    buffer.push_back(match samples {
        AudioSamples::Mono(samples) => AudioSamples::Mono(
            samples
                .into_iter()
                .enumerate()
                .map(|(i, s)| s * fade_out(i))
                .collect(),
        ),
        AudioSamples::Stereo(samples) => AudioSamples::Stereo(
            samples
                .into_iter()
                .enumerate()
                .map(|(i, (l, r))| (l * fade_out(i), r * fade_out(i)))
                .collect(),
        ),
    });
}

/// Crossfade from `from` to `to` (both of the same length) with complementary raised cosine
/// gains.
fn crossfade_samples(from: AudioSamples, to: AudioSamples) -> AudioSamples {
    let len = from.len();
    match (from, to) {
        (AudioSamples::Mono(from), AudioSamples::Mono(to)) => AudioSamples::Mono(
            from.into_iter()
                .zip(to)
                .enumerate()
                .map(|(i, (a, b))| a + (b - a) * fade_in_gain(i, len))
                .collect(),
        ),
        (AudioSamples::Stereo(from), AudioSamples::Stereo(to)) => AudioSamples::Stereo(
            from.into_iter()
                .zip(to)
                .enumerate()
                .map(|(i, ((al, ar), (bl, br)))| {
                    let gain = fade_in_gain(i, len);
                    (al + (bl - al) * gain, ar + (br - ar) * gain)
                })
                .collect(),
        ),
        (from, _) => {
            error!("Wrong channel layout");
            from
        }
    }
}

/// Raised cosine rising from 0.0 to 1.0 over `len` frames.
fn fade_in_gain(frame: usize, len: usize) -> f64 {
    0.5 * (1.0 - (PI * (frame as f64 + 0.5) / len as f64).cos())
}

/// Sum of all channels for `len` frames starting at `start`.
fn mid_samples(buffer: &AudioSamplesBuffer, start: usize, len: usize) -> Vec<f64> {
    let mut mid = vec![0.0; len];
    let mut channel = vec![0.0; len];
    for channel_index in 0..buffer.channels() {
        buffer.copy_from_channel_to_slice(channel_index, start, &mut channel);
        mid.iter_mut().zip(&channel).for_each(|(m, s)| *m += s);
    }
    mid
}
