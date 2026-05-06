use std::time::Duration;

use super::test_utils::*;
use super::*;

const RATE: u32 = 48_000;
const SAMPLE48: Duration = Duration::from_nanos(1_000_000_000 / 48_000);

fn mono(samples: AudioSamples) -> Vec<f64> {
    let AudioSamples::Mono(s) = samples else {
        panic!("expected Mono output");
    };
    s
}

/// Steady state: equal input/output rates, 500ms of contiguous input,
/// request a 100ms window well after warmup. Output should match the
/// signal source at the same PTS, sample-for-sample (within tolerance).
#[test]
fn steady_state_matched_rates() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::ZERO).unwrap();

    let batch = Duration::from_millis(20);
    for i in 0..25_u32 {
        r.write_batch(source.batch(batch * i, batch));
    }

    let out_start = Duration::from_millis(100);
    let samples = mono(r.get_samples((out_start, Duration::from_millis(200))));
    assert_eq!(samples.len(), 4_800);

    SignalAssertion {
        output: &samples,
        output_window: 0..samples.len(),
        source: &source,
        source_pts_at_window_start: out_start + SAMPLE48,
        tolerance: 0.01,
    }
    .assert();
}

// --- Clean state, single get_samples call -------------------------------

/// Input covers [50ms, 200ms); request [0, 20ms) — entirely before the
/// buffered range. Expectation: 20ms of silence.
#[test]
fn request_before_buffered_input_returns_silence() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }

    let samples = mono(r.get_samples((Duration::from_millis(0), Duration::from_millis(20))));
    assert_eq!(samples.len(), 960);

    SignalAssertion {
        output: &samples,
        output_window: 0..samples.len(),
        source: &SignalSource::new(RATE, silence()),
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
}

/// Input covers [50ms, 200ms); request [100ms, 120ms) — fully inside the
/// buffered range. Expectation: 20ms of audio matching the source at the
/// same PTS (modulo the constant filter-latency offset).
#[test]
fn request_inside_buffered_input_returns_aligned_audio() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }

    let out_start = Duration::from_millis(100);
    let samples = mono(r.get_samples((out_start, Duration::from_millis(120))));
    assert_eq!(samples.len(), 960);

    SignalAssertion {
        output: &samples,
        output_window: 0..samples.len(),
        source: &source,
        source_pts_at_window_start: out_start + SAMPLE48,
        tolerance: 0.01,
    }
    .assert();
}

/// Input covers [50ms, 200ms); request [400ms, 420ms) — entirely past the
/// buffered range. Expectation: 20ms of silence. (Note: the *current*
/// resampler also destructively drains the buffered data here.)
#[test]
fn request_past_buffered_input_returns_silence() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }

    let samples = mono(r.get_samples((Duration::from_millis(400), Duration::from_millis(420))));
    assert_eq!(samples.len(), 960);

    SignalAssertion {
        output: &samples,
        output_window: 0..samples.len(),
        source: &SignalSource::new(RATE, silence()),
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
}

// --- Clean state, two consecutive get_samples calls ---------------------

/// Two consecutive pre-input requests return silence and don't corrupt
/// internal state.
#[test]
fn consecutive_pre_input_requests_return_silence() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }

    let first = mono(r.get_samples((Duration::from_millis(0), Duration::from_millis(20))));
    let second = mono(r.get_samples((Duration::from_millis(20), Duration::from_millis(40))));

    let silent = SignalSource::new(RATE, silence());
    SignalAssertion {
        output: &first,
        output_window: 0..first.len(),
        source: &silent,
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
    SignalAssertion {
        output: &second,
        output_window: 0..second.len(),
        source: &silent,
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
}

/// Two consecutive inside-input requests return contiguous, aligned audio.
#[test]
fn consecutive_inside_input_requests_return_contiguous_audio() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }

    let first_start = Duration::from_millis(100);
    let second_start = Duration::from_millis(120);
    let first = mono(r.get_samples((first_start, Duration::from_millis(120))));
    let second = mono(r.get_samples((second_start, Duration::from_millis(140))));

    SignalAssertion {
        output: &first,
        output_window: 0..first.len(),
        source: &source,
        source_pts_at_window_start: first_start + SAMPLE48,
        tolerance: 0.01,
    }
    .assert();
    SignalAssertion {
        output: &second,
        output_window: 0..second.len(),
        source: &source,
        source_pts_at_window_start: second_start + SAMPLE48,
        tolerance: 0.01,
    }
    .assert();
}

/// Two consecutive past-input requests both return silence.
#[test]
fn consecutive_past_input_requests_return_silence() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }

    let _ = r.get_samples((Duration::from_millis(400), Duration::from_millis(420)));
    let second = mono(r.get_samples((Duration::from_millis(420), Duration::from_millis(440))));

    SignalAssertion {
        output: &second,
        output_window: 0..second.len(),
        source: &SignalSource::new(RATE, silence()),
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
}

// --- Drained state, then write a batch and request [440ms, 460ms) -------
//
// The resampler is first driven into the "drained" state by writing 150ms
// of input ([50ms, 200ms)) and then making two `get_samples` calls past
// that range — by the time we add a new batch, the buffered input has
// already been destructively dropped and `before_first_resample` is false.
// We then add one new batch and ask for [440ms, 460ms). The ideal behavior
// is: only the input frames that overlap [440ms, 460ms) appear in the
// output, at their correct PTS; everything else is silence.

/// New batch [430ms, 435ms) — entirely before the request window. Ideally:
/// 20ms of silence (no input overlaps the request).
#[test]
fn recovery_with_input_before_request_window() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }
    let _ = r.get_samples((Duration::from_millis(400), Duration::from_millis(420)));
    let _ = r.get_samples((Duration::from_millis(420), Duration::from_millis(440)));

    r.write_batch(source.batch(Duration::from_millis(430), Duration::from_millis(5)));
    let samples = mono(r.get_samples((Duration::from_millis(440), Duration::from_millis(460))));
    assert_eq!(samples.len(), 960);

    SignalAssertion {
        output: &samples,
        output_window: 0..samples.len(),
        source: &SignalSource::new(RATE, silence()),
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
}

/// New batch [435ms, 450ms) — partial overlap with start of request window.
/// Ideally:
/// - output[0..480]   = audio at input [440ms, 450ms)
/// - output[480..960] = silence (no input for [450ms, 460ms))
#[test]
fn recovery_with_input_overlapping_request_start() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }
    let _ = r.get_samples((Duration::from_millis(400), Duration::from_millis(420)));
    let _ = r.get_samples((Duration::from_millis(420), Duration::from_millis(440)));

    r.write_batch(source.batch(Duration::from_millis(435), Duration::from_millis(15)));
    let samples = mono(r.get_samples((Duration::from_millis(440), Duration::from_millis(460))));
    assert_eq!(samples.len(), 960);

    SignalAssertion {
        output: &samples,
        output_window: 0..480,
        source: &source,
        source_pts_at_window_start: Duration::from_millis(440) + SAMPLE48,
        tolerance: 0.01,
    }
    .assert();
    SignalAssertion {
        output: &samples,
        output_window: 480..960,
        source: &SignalSource::new(RATE, silence()),
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
}

/// New batch [445ms, 455ms) — entirely inside the request window. Ideally:
/// - output[0..240]   = silence (no input for [440ms, 445ms))
/// - output[240..720] = audio at input [445ms, 455ms)
/// - output[720..960] = silence (no input for [455ms, 460ms))
#[test]
fn recovery_with_input_inside_request_window() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }
    let _ = r.get_samples((Duration::from_millis(400), Duration::from_millis(420)));
    let _ = r.get_samples((Duration::from_millis(420), Duration::from_millis(440)));

    r.write_batch(source.batch(Duration::from_millis(445), Duration::from_millis(10)));
    let samples = mono(r.get_samples((Duration::from_millis(440), Duration::from_millis(460))));
    assert_eq!(samples.len(), 960);

    let silent = SignalSource::new(RATE, silence());
    SignalAssertion {
        output: &samples,
        output_window: 0..240,
        source: &silent,
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
    SignalAssertion {
        output: &samples,
        output_window: 240..720,
        source: &source,
        source_pts_at_window_start: Duration::from_millis(445) + SAMPLE48,
        tolerance: 0.01,
    }
    .assert();
    SignalAssertion {
        output: &samples,
        output_window: 720..960,
        source: &silent,
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
}

/// New batch [455ms, 470ms) — partial overlap with end of request window.
/// Ideally:
/// - output[0..720]   = silence (no input for [440ms, 455ms))
/// - output[720..960] = audio at input [455ms, 460ms)
#[test]
fn recovery_with_input_overlapping_request_end() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r =
        InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(50)).unwrap();
    let batch_dur = Duration::from_millis(10);
    for i in 0..15_u32 {
        r.write_batch(source.batch(Duration::from_millis(50) + batch_dur * i, batch_dur));
    }
    let _ = r.get_samples((Duration::from_millis(400), Duration::from_millis(420)));
    let _ = r.get_samples((Duration::from_millis(420), Duration::from_millis(440)));

    r.write_batch(source.batch(Duration::from_millis(455), Duration::from_millis(15)));
    let samples = mono(r.get_samples((Duration::from_millis(440), Duration::from_millis(460))));
    assert_eq!(samples.len(), 960);

    SignalAssertion {
        output: &samples,
        output_window: 0..720,
        source: &SignalSource::new(RATE, silence()),
        source_pts_at_window_start: Duration::ZERO,
        tolerance: 1e-3,
    }
    .assert();
    SignalAssertion {
        output: &samples,
        output_window: 720..960,
        source: &source,
        source_pts_at_window_start: Duration::from_millis(455) + SAMPLE48,
        tolerance: 0.01,
    }
    .assert();
}
