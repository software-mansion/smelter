use std::time::Duration;

use audioadapter::Adapter;

use crate::{AudioSamples, utils::AudioSamplesBuffer};

use super::splice::{self, CROSSFADE_DURATION};

/// Length of the blocks whose energy decides whether audio is silent.
const BLOCK_DURATION: Duration = Duration::from_millis(5);

/// How far from the front of the input buffer silence is searched for.
const SEARCH_DURATION: Duration = Duration::from_millis(100);

/// Energy (dBFS) of the louder channel below which a block is silent, at the lowest and highest
/// strength.
const SILENCE_THRESHOLD_DB: (f64, f64) = (-60.0, -40.0);

/// Shortest silent run that can be cut, at the lowest and highest strength.
const MIN_RUN_DURATION: (Duration, Duration) =
    (Duration::from_millis(80), Duration::from_millis(20));

/// Silence left untouched at both ends of the run, keeps decay tails and onsets intact. At the
/// lowest and highest strength.
const MARGIN_DURATION: (Duration, Duration) = (Duration::from_millis(10), Duration::from_millis(5));

/// Remove up to `max_frames` frames from the middle of the first silent run near the front of
/// `buffer`, joining both sides of the cut with a crossfade. `strength` (0.0 to 1.0) relaxes
/// what counts as a silent run.
///
/// Returns the number of removed frames, or `None` (leaving `buffer` untouched) if there is no
/// silent run long enough for a cut.
pub(super) fn cut_silence(
    buffer: &mut AudioSamplesBuffer,
    max_frames: usize,
    strength: f64,
    sample_rate: u32,
) -> Option<usize> {
    let frames =
        |duration: Duration| (duration.as_secs_f64() * sample_rate as f64).round() as usize;
    let interpolate = |(low, high): (f64, f64)| low + (high - low) * strength;
    let interpolate_frames = |(low, high): (Duration, Duration)| {
        interpolate((frames(low) as f64, frames(high) as f64)).round() as usize
    };

    let crossfade = frames(CROSSFADE_DURATION);
    if max_frames < crossfade {
        return None;
    }
    let threshold = 10f64.powf(interpolate(SILENCE_THRESHOLD_DB) / 10.0);
    let min_run = interpolate_frames(MIN_RUN_DURATION);
    let margin = interpolate_frames(MARGIN_DURATION);

    let window = buffer.read_samples(usize::min(frames(SEARCH_DURATION), buffer.frames()));
    let Some((run_start, run_end)) =
        find_silent_run(&window, frames(BLOCK_DURATION), threshold, min_run)
    else {
        buffer.push_front(window);
        return None;
    };

    // Cut region and the crossfade after it have to fit between the margins.
    let usable = (run_end - run_start).saturating_sub(2 * margin + crossfade);
    let cut = usize::min(max_frames, usable);
    if cut < crossfade {
        buffer.push_front(window);
        return None;
    }
    let cut_start = run_start + margin + (usable - cut) / 2;

    let mut window = AudioSamplesBuffer::from(window);
    let head = window.read_samples(cut_start);
    let from = window.read_samples(crossfade);
    window.drain_samples(cut - crossfade);
    let to = window.read_samples(crossfade);
    let tail = window.read_samples(window.frames());
    buffer.push_front(tail);
    buffer.push_front(splice::crossfade_samples(from, to));
    buffer.push_front(head);
    Some(cut)
}

/// Frame range of the first run of silent blocks of at least `min_run` frames.
fn find_silent_run(
    samples: &AudioSamples,
    block: usize,
    threshold: f64,
    min_run: usize,
) -> Option<(usize, usize)> {
    let blocks = samples.len() / block;
    let mut run_start = None;
    for index in 0..=blocks {
        let is_silent = index < blocks && block_power(samples, index * block, block) < threshold;
        match (is_silent, run_start) {
            (true, None) => run_start = Some(index * block),
            (false, Some(start)) => {
                let end = index * block;
                if end - start >= min_run {
                    return Some((start, end));
                }
                run_start = None;
            }
            _ => {}
        }
    }
    None
}

/// Mean square of the louder channel over `len` frames starting at `start`.
fn block_power(samples: &AudioSamples, start: usize, len: usize) -> f64 {
    let sum = match samples {
        AudioSamples::Mono(samples) => samples[start..start + len].iter().map(|s| s * s).sum(),
        AudioSamples::Stereo(samples) => {
            let (l, r) = samples[start..start + len]
                .iter()
                .fold((0.0, 0.0), |(l, r), (a, b)| (l + a * a, r + b * b));
            f64::max(l, r)
        }
    };
    sum / len as f64
}
