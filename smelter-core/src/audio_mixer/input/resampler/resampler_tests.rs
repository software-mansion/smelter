use std::ops::Range;
use std::time::Duration;

use super::test_utils::*;
use super::*;

const RATE: u32 = 48_000;
const SAMPLE48: Duration = Duration::from_nanos(1_000_000_000 / 48_000);

// ============================ Per-test helpers ============================

fn fresh_resampler(first_pts: Duration) -> InputResampler {
    InputResampler::new(RATE, RATE, AudioChannels::Mono, first_pts).unwrap()
}

fn mono(samples: AudioSamples) -> Vec<f64> {
    let AudioSamples::Mono(s) = samples else {
        panic!("expected Mono output");
    };
    s
}

/// Write contiguous batches from `source` covering `[start, end)`. The last
/// batch may be shorter than `batch_dur` if `(end - start)` doesn't divide
/// evenly.
fn write_contiguous(
    resampler: &mut InputResampler,
    source: &SignalSource,
    start: Duration,
    end: Duration,
    batch_dur: Duration,
) {
    let mut t = start;
    while t < end {
        let next = (t + batch_dur).min(end);
        resampler.write_batch(source.batch(t, next - t));
        t = next;
    }
}

/// Drive the resampler into the "drained" state: 150ms of input written from
/// PTS 50ms..200ms, then two `get_samples` calls past that range. After this
/// returns, the buffered input has been destructively dropped and
/// `before_first_resample` is false — the state in which the resampler has
/// to recover gracefully when fresh input eventually arrives.
fn drive_drained_state(resampler: &mut InputResampler, source: &SignalSource) {
    write_contiguous(
        resampler,
        source,
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_millis(10),
    );
    let _ = resampler.get_samples((Duration::from_millis(400), Duration::from_millis(420)));
    let _ = resampler.get_samples((Duration::from_millis(420), Duration::from_millis(440)));
}

/// Tolerance for "this region must be silence". Non-zero to allow filter
/// ringing right after a non-zero input has been processed.
const SILENCE_TOL: f64 = 1e-3;
/// Tolerance for "this region must reproduce the source signal".
const SIGNAL_TOL: f64 = 0.01;

fn assert_silence(output: &[f64], window: Range<usize>) {
    let silent = SignalSource::new(RATE, silence());
    SignalAssertion {
        output,
        output_window: window,
        source: &silent,
        source_pts_at_window_start: Duration::ZERO,
        tolerance: SILENCE_TOL,
    }
    .assert();
}

fn assert_signal(
    output: &[f64],
    window: Range<usize>,
    source: &SignalSource,
    source_pts_at_window_start: Duration,
) {
    SignalAssertion {
        output,
        output_window: window,
        source,
        source_pts_at_window_start,
        tolerance: SIGNAL_TOL,
    }
    .assert();
}

// =============================== Tests ===============================

/// Steady state: equal input/output rates, 500ms of contiguous input,
/// request a 100ms window well after warmup. Output should match the
/// signal source at the same PTS, sample-for-sample (within tolerance).
#[test]
fn steady_state_matched_rates() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::ZERO);

    let batch = Duration::from_millis(20);
    for i in 0..25_u32 {
        r.write_batch(source.batch(batch * i, batch));
    }

    let out_start = Duration::from_millis(100);
    let out_end = Duration::from_millis(200);
    let samples = mono(r.get_samples((out_start, out_end)));
    assert_eq!(samples.len(), 4_800);

    assert_signal(&samples, 0..samples.len(), &source, out_start + SAMPLE48);
}

// --- Clean state, single get_samples call -------------------------------

/// Input covers [50ms, 200ms); request [0, 20ms) — entirely before the
/// buffered range. Expectation: 20ms of silence.
#[test]
fn request_before_buffered_input_returns_silence() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    write_contiguous(
        &mut r,
        &source,
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_millis(10),
    );

    let samples = mono(r.get_samples((Duration::from_millis(0), Duration::from_millis(20))));
    assert_eq!(samples.len(), 960);

    assert_silence(&samples, 0..samples.len());
}

/// Input covers [50ms, 200ms); request [100ms, 120ms) — fully inside the
/// buffered range. Expectation: 20ms of audio matching the source at the
/// same PTS (modulo the constant filter-latency offset).
#[test]
fn request_inside_buffered_input_returns_aligned_audio() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    write_contiguous(
        &mut r,
        &source,
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_millis(10),
    );

    let out_start = Duration::from_millis(100);
    let samples = mono(r.get_samples((out_start, Duration::from_millis(120))));
    assert_eq!(samples.len(), 960);

    assert_signal(&samples, 0..samples.len(), &source, out_start + SAMPLE48);
}

/// Input covers [50ms, 200ms); request [400ms, 420ms) — entirely past the
/// buffered range. Expectation: 20ms of silence. (Note: the *current*
/// resampler also destructively drains the buffered data here.)
#[test]
fn request_past_buffered_input_returns_silence() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    write_contiguous(
        &mut r,
        &source,
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_millis(10),
    );

    let samples = mono(r.get_samples((Duration::from_millis(400), Duration::from_millis(420))));
    assert_eq!(samples.len(), 960);

    assert_silence(&samples, 0..samples.len());
}

// --- Clean state, two consecutive get_samples calls ---------------------

/// Two consecutive pre-input requests return silence and don't corrupt
/// internal state.
#[test]
fn consecutive_pre_input_requests_return_silence() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    write_contiguous(
        &mut r,
        &source,
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_millis(10),
    );

    let first = mono(r.get_samples((Duration::from_millis(0), Duration::from_millis(20))));
    let second = mono(r.get_samples((Duration::from_millis(20), Duration::from_millis(40))));

    assert_silence(&first, 0..first.len());
    assert_silence(&second, 0..second.len());
}

/// Two consecutive inside-input requests return contiguous, aligned audio.
#[test]
fn consecutive_inside_input_requests_return_contiguous_audio() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    write_contiguous(
        &mut r,
        &source,
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_millis(10),
    );

    let first_start = Duration::from_millis(100);
    let second_start = Duration::from_millis(120);
    let first = mono(r.get_samples((first_start, Duration::from_millis(120))));
    let second = mono(r.get_samples((second_start, Duration::from_millis(140))));

    assert_signal(&first, 0..first.len(), &source, first_start + SAMPLE48);
    assert_signal(&second, 0..second.len(), &source, second_start + SAMPLE48);
}

/// Two consecutive past-input requests both return silence.
#[test]
fn consecutive_past_input_requests_return_silence() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    write_contiguous(
        &mut r,
        &source,
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_millis(10),
    );

    let _ = r.get_samples((Duration::from_millis(400), Duration::from_millis(420)));
    let second = mono(r.get_samples((Duration::from_millis(420), Duration::from_millis(440))));

    assert_silence(&second, 0..second.len());
}

// --- Drained state, then write a batch and request [440ms, 460ms) -------
//
// The resampler is first driven into the drained state by `drive_drained_state`.
// We then add one new batch and ask for [440ms, 460ms). The ideal behavior
// is: only the input frames that overlap [440ms, 460ms) appear in the
// output, at their correct PTS; everything else is silence.

/// New batch [430ms, 435ms) — entirely before the request window. Ideally:
/// 20ms of silence (no input overlaps the request).
#[test]
fn recovery_with_input_before_request_window() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    drive_drained_state(&mut r, &source);

    r.write_batch(source.batch(Duration::from_millis(430), Duration::from_millis(5)));
    let samples = mono(r.get_samples((Duration::from_millis(440), Duration::from_millis(460))));
    assert_eq!(samples.len(), 960);

    assert_silence(&samples, 0..samples.len());
}

/// New batch [435ms, 450ms) — partial overlap with start of request window.
/// Ideally:
/// - output[0..480]   = audio at input [440ms, 450ms)
/// - output[480..960] = silence (no input for [450ms, 460ms))
#[test]
fn recovery_with_input_overlapping_request_start() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    drive_drained_state(&mut r, &source);

    r.write_batch(source.batch(Duration::from_millis(435), Duration::from_millis(15)));
    let samples = mono(r.get_samples((Duration::from_millis(440), Duration::from_millis(460))));
    assert_eq!(samples.len(), 960);

    assert_signal(&samples, 0..480, &source, Duration::from_millis(440) + SAMPLE48);
    assert_silence(&samples, 480..960);
}

/// New batch [445ms, 455ms) — entirely inside the request window. Ideally:
/// - output[0..240]   = silence (no input for [440ms, 445ms))
/// - output[240..720] = audio at input [445ms, 455ms)
/// - output[720..960] = silence (no input for [455ms, 460ms))
#[test]
fn recovery_with_input_inside_request_window() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    drive_drained_state(&mut r, &source);

    r.write_batch(source.batch(Duration::from_millis(445), Duration::from_millis(10)));
    let samples = mono(r.get_samples((Duration::from_millis(440), Duration::from_millis(460))));
    assert_eq!(samples.len(), 960);

    assert_silence(&samples, 0..240);
    assert_signal(&samples, 240..720, &source, Duration::from_millis(445) + SAMPLE48);
    assert_silence(&samples, 720..960);
}

/// New batch [455ms, 470ms) — partial overlap with end of request window.
/// Ideally:
/// - output[0..720]   = silence (no input for [440ms, 455ms))
/// - output[720..960] = audio at input [455ms, 460ms)
#[test]
fn recovery_with_input_overlapping_request_end() {
    let source = SignalSource::new(RATE, chirp(200.0, 1000.0));
    let mut r = fresh_resampler(Duration::from_millis(50));
    drive_drained_state(&mut r, &source);

    r.write_batch(source.batch(Duration::from_millis(455), Duration::from_millis(15)));
    let samples = mono(r.get_samples((Duration::from_millis(440), Duration::from_millis(460))));
    assert_eq!(samples.len(), 960);

    assert_silence(&samples, 0..720);
    assert_signal(&samples, 720..960, &source, Duration::from_millis(455) + SAMPLE48);
}
