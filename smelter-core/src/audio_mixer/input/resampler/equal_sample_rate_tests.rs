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

/// Not a real test — just dumps 5 seconds of the default `test_signal()`
/// to a WAV file so its waveform can be inspected outside the test runner.
/// Always passes.
#[test]
fn dump_test_signal() {
    let source = SignalSource::new(RATE, test_signal());
    let samples = source.samples(D, Duration::from_secs(1) + D);
    dump_wav(&samples, RATE, "test_signal.wav");
}

/// First `get_samples` call on a freshly-constructed resampler — the
/// `before_first_resample` gate is still set, so every test in this module
/// exercises one branch of `maybe_prepare_before_resample`. Tests are
/// ordered by the position of the buffered input relative to the request
/// window: way before → straddling start → covering → straddling end →
/// way after.
///
/// All PTS values are perturbed by [`D`] so we don't accidentally rely on
/// round-millisecond timestamps.
///
/// Every test's *first* assertion skips the leading 5 output samples. On
/// the very first resample the rubato FIR filter is fed against either
/// zero-padded history (warmup) or the freshly-prepared input buffer, and
/// the first handful of samples carry a small transient that
/// `samples_to_drop` doesn't fully hide. After ~5 samples the output has
/// settled and matches the source (or silence) cleanly. Subsequent
/// asserts in the same test compare windows further into the buffer and
/// don't need this guard.
mod fresh {
    use super::*;

    /// Input [0, 20ms), request [40ms, 60ms). Input entirely before the
    /// request window — drain consumes the full buffer; output is silence.
    #[test]
    fn input_before_request() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono, D).unwrap();

        r.write_batch(source.batch(D, Duration::from_millis(10)));
        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(10)));

        let samples = mono(
            r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)),
        );
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_before_request.wav");

        SignalAssertion {
            output: &samples,
            output_window: 5..samples.len(),
            source: &SignalSource::new(RATE, silence()),
            source_pts_at_window_start: Duration::ZERO,
            tolerance: 1e-3,
        }
        .assert();
    }

    /// Input [0, 30ms), request [20ms, 40ms). Input overlaps only the
    /// start of the request — drain shaves off [0, 20ms); the suffix
    /// [30ms, 40ms) has no input.
    ///
    /// Ideal output:
    /// - output[0..480]   = audio at input [20ms, 30ms)
    /// - output[480..960] = silence
    #[test]
    fn input_overlaps_request_start() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono, D).unwrap();

        r.write_batch(source.batch(D, Duration::from_millis(10)));
        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_overlaps_request_start.wav");

        // Skip the leading 5 samples (FIR transient — see module doc)
        // and the trailing 10 samples of the audio portion (FIR averaging
        // in padded zeros across the audio→silence boundary).
        SignalAssertion {
            output: &samples,
            output_window: 5..470,
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48 * 6,
            tolerance: 0.01,
        }
        .assert();
        // Skip 10 samples on either side of the audio→silence boundary:
        // the sinc resampler's FIR filter aliases real input across the
        // boundary into the "should be silent" region as ringing, and
        // the trailing edge of the buffer goes through a partial-resample
        // re-warmup that injects a transient. Neither shows up more than
        // a handful of samples in.
        SignalAssertion {
            output: &samples,
            output_window: 490..950,
            source: &SignalSource::new(RATE, silence()),
            source_pts_at_window_start: Duration::ZERO,
            tolerance: 1e-3,
        }
        .assert();
    }

    /// Input [10ms, 50ms), request [20ms, 40ms). Input fully covers the
    /// request — drain shaves off the [10ms, 20ms) prefix; subsequent
    /// resample iterations sit in the on-time dead-band. Output should
    /// reproduce the source at the requested PTS.
    #[test]
    fn input_covers_request() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r =
            InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(10) + D)
                .unwrap();

        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(30) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_covers_request.wav");

        SignalAssertion {
            output: &samples,
            output_window: 5..samples.len(),
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48 * 6,
            tolerance: 0.01,
        }
        .assert();
    }

    /// Same as [`input_covers_request`] but with the input batches aligned
    /// exactly to the request grid: [0, 20ms), [20ms, 40ms), [40ms, 60ms).
    /// The drain stops on a clean batch boundary instead of mid-batch.
    #[test]
    fn input_covers_request_grid_aligned() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono, D).unwrap();

        r.write_batch(source.batch(D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(20) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(40) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_covers_request_grid_aligned.wav");

        SignalAssertion {
            output: &samples,
            output_window: 5..samples.len(),
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48 * 6,
            tolerance: 0.01,
        }
        .assert();
    }

    /// Same as [`input_starts_at_request_start`] but the input is shifted
    /// **backward by 0.5ms** (still well below `SHIFT_THRESHOLD = 2ms`).
    /// `maybe_prepare_before_resample` drains 24 too-old samples from
    /// the front of the buffer, restoring alignment; the main loop
    /// stays in the on-time dead-band.
    ///
    /// Output should reproduce the source. Skip the first 5 samples to
    /// avoid the FIR transient at the buffer-prefix-drain boundary.
    #[test]
    fn input_shifted_backward_within_threshold() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let shift = Duration::from_micros(500);
        let first_pts = Duration::from_millis(20) - shift + D;
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono, first_pts).unwrap();

        r.write_batch(source.batch(first_pts, Duration::from_millis(20)));
        r.write_batch(
            source.batch(Duration::from_millis(40) - shift + D, Duration::from_millis(20)),
        );

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_shifted_backward_within_threshold.wav");

        SignalAssertion {
            output: &samples,
            output_window: 5..960,
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48 * 6,
            tolerance: 0.01,
        }
        .assert();
    }

    /// Input starts exactly at request start: input [20ms, 60ms),
    /// request [20ms, 40ms). `input_buffer_start_pts == pts_range.0`, so
    /// neither the drain nor the pad branch in
    /// `maybe_prepare_before_resample` fires. The whole resample loop
    /// runs in the on-time dead-band.
    #[test]
    fn input_starts_at_request_start() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(20) + D)
            .unwrap();

        r.write_batch(source.batch(Duration::from_millis(20) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(40) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_starts_at_request_start.wav");

        SignalAssertion {
            output: &samples,
            output_window: 5..samples.len(),
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48 * 6,
            tolerance: 0.01,
        }
        .assert();
    }

    /// Same as [`input_starts_at_request_start`] but the input is shifted
    /// **forward by 0.5ms** (still well below `SHIFT_THRESHOLD = 2ms`).
    /// `maybe_prepare_before_resample` pads 24 silent samples at the
    /// front of the buffer to align the timeline; the main loop stays
    /// in the on-time dead-band (no stretch/squash applied).
    ///
    /// Output: 24 silent samples followed by the source signal. We skip
    /// the FIR transition window around the silence→audio boundary.
    #[test]
    fn input_shifted_forward_within_threshold() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let shift = Duration::from_micros(500);
        let first_pts = Duration::from_millis(20) + shift + D;
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono, first_pts).unwrap();

        r.write_batch(source.batch(first_pts, Duration::from_millis(20)));
        r.write_batch(
            source.batch(Duration::from_millis(40) + shift + D, Duration::from_millis(20)),
        );

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_shifted_forward_within_threshold.wav");

        // Front of buffer is the 24 silent padding samples — assert only
        // the leading edge before any FIR smearing can pull real signal in.
        SignalAssertion {
            output: &samples,
            output_window: 0..5,
            source: &SignalSource::new(RATE, silence()),
            source_pts_at_window_start: Duration::ZERO,
            tolerance: 1e-3,
        }
        .assert();
        // Skip the FIR transition (24-sample padding plus ~16 taps of
        // smearing); after that the output reproduces the source.
        SignalAssertion {
            output: &samples,
            output_window: 50..960,
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48 * 51,
            tolerance: 0.01,
        }
        .assert();
    }

    /// Input [30ms, 70ms), request [20ms, 40ms). Input overlaps only the
    /// end of the request — `maybe_prepare_before_resample` pads the
    /// front of the buffer with silence so the timeline lines up.
    ///
    /// Ideal output:
    /// - output[0..480]   = silence
    /// - output[480..960] = audio at input [30ms, 40ms)
    #[test]
    fn input_overlaps_request_end() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r =
            InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(30) + D)
                .unwrap();

        r.write_batch(source.batch(Duration::from_millis(30) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_overlaps_request_end.wav");

        // Skip 15 samples at the front (5 for the per-fresh-test FIR
        // transient — see module doc — plus 10 more for the silence→audio
        // FIR ringing) and 10 at the trailing edge (sinc smearing from
        // real input across the boundary).
        SignalAssertion {
            output: &samples,
            output_window: 15..470,
            source: &SignalSource::new(RATE, silence()),
            source_pts_at_window_start: Duration::ZERO,
            tolerance: 1e-3,
        }
        .assert();
        // Skip the leading 10 samples of the audio portion: as the FIR
        // window slides across the silence→audio boundary it averages in
        // the front-padded zeros, so the first few samples ramp up from
        // silence rather than land cleanly on the source.
        SignalAssertion {
            output: &samples,
            output_window: 490..960,
            source: &source,
            source_pts_at_window_start: Duration::from_millis(30) + D + SAMPLE48 * 11,
            tolerance: 0.01,
        }
        .assert();
    }

    /// Input [60ms, 80ms), request [20ms, 40ms). Input entirely after the
    /// request — `maybe_prepare_before_resample` returns silence directly
    /// without engaging the resampler.
    #[test]
    fn input_after_request() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r =
            InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(60) + D)
                .unwrap();

        r.write_batch(source.batch(Duration::from_millis(60) + D, Duration::from_millis(10)));
        r.write_batch(source.batch(Duration::from_millis(70) + D, Duration::from_millis(10)));

        let samples = mono(
            r.get_samples((Duration::from_millis(20) + D, Duration::from_millis(40) + D)),
        );
        assert_eq!(samples.len(), 960);
        dump_wav(&samples, RATE, "fresh_input_after_request.wav");

        SignalAssertion {
            output: &samples,
            output_window: 5..samples.len(),
            source: &SignalSource::new(RATE, silence()),
            source_pts_at_window_start: Duration::ZERO,
            tolerance: 1e-3,
        }
        .assert();
    }
}

/// Second `get_samples` call on a resampler that has already been driven
/// past the `before_first_resample` gate. The common setup writes two
/// 20ms batches ([10ms, 50ms)+D) and calls `get_samples((20ms, 40ms)+D)`,
/// leaving the resampler with ~8.667ms of input buffered ([41.333ms,
/// 50ms)+D) plus a few leftover output frames in `output_buffer`.
///
/// Each test then writes a *different* batch ahead of the previous data
/// and calls `get_samples((40ms, 60ms)+D)`. The concatenated output of
/// both `get_samples` calls is dumped to a WAV file for inspection.
mod running {
    use super::*;

    /// Common init: [10, 50)+D worth of input, then `get_samples((20, 40)+D)`.
    fn primed() -> (SignalSource, InputResampler, Vec<f64>) {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r =
            InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(10) + D)
                .unwrap();
        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(30) + D, Duration::from_millis(20)));
        let first = mono(
            r.get_samples((Duration::from_millis(20) + D, Duration::from_millis(40) + D)),
        );
        (source, r, first)
    }

    /// Run the second `get_samples` and dump the concatenated [20ms, 60ms)
    /// output to `running_<name>` for inspection.
    fn run_and_dump(r: &mut InputResampler, first: &[f64], name: &str) -> Vec<f64> {
        let second = mono(
            r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)),
        );
        assert_eq!(second.len(), 960);
        let mut combined = Vec::with_capacity(first.len() + second.len());
        combined.extend_from_slice(first);
        combined.extend_from_slice(&second);
        dump_wav(&combined, RATE, &format!("running_{name}"));
        second
    }

    // FLAG: input_before_request — can't make the buffer entirely before
    // the request. After init the buffer holds [41.333+D, 50+D) and
    // write_batch will not retroactively shift that backwards.

    /// Append a small contiguous batch [50ms, 55ms)+D — buffer ends inside
    /// the request window. Analogous to `fresh::input_overlaps_request_start`.
    #[test]
    fn input_overlaps_request_start() {
        let (source, mut r, first) = primed();
        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(5)));

        let _second = run_and_dump(&mut r, &first, "input_overlaps_request_start.wav");
        // Buffer covers ~13.667ms of the 20ms request; the remainder
        // should be silence. Skip the FIR transition near the audio→silence
        // boundary. (Exact source-PTS alignment skipped for now — visually
        // verify via the dumped WAV.)
        let _ = source;
    }

    /// Append a contiguous batch [50ms, 70ms)+D — buffer extends past the
    /// request end. Analogous to `fresh::input_covers_request`.
    #[test]
    fn input_covers_request() {
        let (source, mut r, first) = primed();
        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(20)));

        let _second = run_and_dump(&mut r, &first, "input_covers_request.wav");
        let _ = source;
    }

    // FLAG: input_covers_request_grid_aligned — the running-state buffer
    // has a fractional start (~41.333ms after init), so the
    // batch-grid-alignment property doesn't transfer.

    /// Append an overlapping batch [49.5ms, 70ms)+D — overlaps existing
    /// buffer end by 0.5ms (sub-`SHIFT_THRESHOLD`). write_batch trusts
    /// the new end_pts; the resampler then sees a small forward overlap
    /// in its input buffer. Analogous to `fresh::input_shifted_backward_within_threshold`.
    #[test]
    fn input_shifted_backward_within_threshold() {
        let (source, mut r, first) = primed();
        let shift = Duration::from_micros(500);
        r.write_batch(source.batch(
            Duration::from_millis(50) - shift + D,
            Duration::from_millis(20),
        ));

        let _second = run_and_dump(
            &mut r,
            &first,
            "input_shifted_backward_within_threshold.wav",
        );
        let _ = source;
    }

    // FLAG: input_starts_at_request_start — running buffer's start is
    // pinned at ~41.333+D ms; cannot be moved exactly to the request start.

    /// Append a small-gap batch [50.5ms, 70.5ms)+D — gap of 0.5ms, well
    /// below `CONTINUITY_THRESHOLD` so write_batch just trusts the new
    /// timestamp without zero-filling. Analogous to
    /// `fresh::input_shifted_forward_within_threshold`.
    #[test]
    fn input_shifted_forward_within_threshold() {
        let (source, mut r, first) = primed();
        let shift = Duration::from_micros(500);
        r.write_batch(source.batch(
            Duration::from_millis(50) + shift + D,
            Duration::from_millis(20),
        ));

        let _second = run_and_dump(
            &mut r,
            &first,
            "input_shifted_forward_within_threshold.wav",
        );
        let _ = source;
    }

    // FLAG: input_overlaps_request_end — running buffer's start is
    // already inside the request (~41.333+D); no write can move the
    // buffer's covered region to be only the back portion of the request.

    // FLAG: input_after_request — would require the buffer to be entirely
    // after the request, but the existing buffered content is inside the
    // request and isn't discarded by write_batch.

    /// Append a batch with a *large* gap [150ms, 170ms)+D — gap of 100ms
    /// > `CONTINUITY_THRESHOLD = 80ms`, so write_batch zero-fills the
    /// missing 100ms. No fresh analog (this exercises the gap-fill branch
    /// of write_batch specifically).
    #[test]
    fn large_gap_append() {
        let (source, mut r, first) = primed();
        r.write_batch(source.batch(Duration::from_millis(150) + D, Duration::from_millis(20)));

        let _second = run_and_dump(&mut r, &first, "large_gap_append.wav");
        let _ = source;
    }
}

/// Third `get_samples` call on a resampler that has been driven through
/// **two** prior requests. The common setup writes [10, 50)+D and calls
/// `get_samples((20, 40)+D)` and `get_samples((40, 60)+D)`, after which:
/// - the input buffer is fully drained (0 frames, `end_pts = 50+D`),
/// - the second `get_samples` over-produced into the squash branch and
///   left ~80 zero frames in `output_buffer` for the next call.
///
/// Each test then writes a *single* batch and calls
/// `get_samples((60, 80)+D)`. The concatenated output of all three
/// `get_samples` calls is dumped to a WAV file for inspection.
///
/// The first ~80 output samples of every test are the leftover zeros
/// from the second init `get_samples` — they appear before the new
/// write's signal can reach the output. Test assertions are deferred
/// (visually verify the dumped WAVs first).
mod drained {
    use super::*;

    /// Common init: [10, 50)+D input, then `get_samples((20, 40)+D)` and
    /// `get_samples((40, 60)+D)`.
    fn primed() -> (SignalSource, InputResampler, Vec<f64>) {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r =
            InputResampler::new(RATE, RATE, AudioChannels::Mono, Duration::from_millis(10) + D)
                .unwrap();
        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(30) + D, Duration::from_millis(20)));
        let first = mono(
            r.get_samples((Duration::from_millis(20) + D, Duration::from_millis(40) + D)),
        );
        let second = mono(
            r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)),
        );
        let mut prev = Vec::with_capacity(first.len() + second.len());
        prev.extend_from_slice(&first);
        prev.extend_from_slice(&second);
        (source, r, prev)
    }

    /// Run the third `get_samples((60, 80)+D)` and dump the concatenated
    /// [20ms, 80ms) output to `drained_<name>` for inspection.
    fn run_and_dump(r: &mut InputResampler, prev: &[f64], name: &str) -> Vec<f64> {
        let third = mono(
            r.get_samples((Duration::from_millis(60) + D, Duration::from_millis(80) + D)),
        );
        assert_eq!(third.len(), 960);
        let mut combined = Vec::with_capacity(prev.len() + third.len());
        combined.extend_from_slice(prev);
        combined.extend_from_slice(&third);
        dump_wav(&combined, RATE, &format!("drained_{name}"));
        third
    }

    /// Write a stale batch [40ms, 50ms)+D — entirely before the request.
    /// Buffer covers [40, 50)+D when get_samples is called. Analogous to
    /// `fresh::input_before_request`.
    #[test]
    fn input_before_request() {
        let (source, mut r, prev) = primed();
        r.write_batch(source.batch(Duration::from_millis(40) + D, Duration::from_millis(10)));

        let _third = run_and_dump(&mut r, &prev, "input_before_request.wav");
        let _ = source;
    }

    /// Write [50ms, 65ms)+D — buffer ends inside the request. Analogous
    /// to `fresh::input_overlaps_request_start`.
    #[test]
    fn input_overlaps_request_start() {
        let (source, mut r, prev) = primed();
        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(15)));

        let _third = run_and_dump(&mut r, &prev, "input_overlaps_request_start.wav");
        let _ = source;
    }

    /// Write [50ms, 90ms)+D — buffer fully covers the request with prefix
    /// and suffix. Analogous to `fresh::input_covers_request`.
    #[test]
    fn input_covers_request() {
        let (source, mut r, prev) = primed();
        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(40)));

        let _third = run_and_dump(&mut r, &prev, "input_covers_request.wav");
        let _ = source;
    }

    /// Write three contiguous batches on the request grid: [40, 60),
    /// [60, 80), [80, 100)+D. Analogous to
    /// `fresh::input_covers_request_grid_aligned`.
    #[test]
    fn input_covers_request_grid_aligned() {
        let (source, mut r, prev) = primed();
        r.write_batch(source.batch(Duration::from_millis(40) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(60) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(80) + D, Duration::from_millis(20)));

        let _third = run_and_dump(&mut r, &prev, "input_covers_request_grid_aligned.wav");
        let _ = source;
    }

    /// Write [59.5ms, 79.5ms)+D — buffer starts 0.5ms *before* the request
    /// (sub-`SHIFT_THRESHOLD`). Analogous to
    /// `fresh::input_shifted_backward_within_threshold`.
    #[test]
    fn input_shifted_backward_within_threshold() {
        let (source, mut r, prev) = primed();
        let shift = Duration::from_micros(500);
        r.write_batch(source.batch(
            Duration::from_millis(60) - shift + D,
            Duration::from_millis(20),
        ));

        let _third = run_and_dump(
            &mut r,
            &prev,
            "input_shifted_backward_within_threshold.wav",
        );
        let _ = source;
    }

    /// Write [60ms, 80ms)+D — buffer starts exactly at the request.
    /// Analogous to `fresh::input_starts_at_request_start`.
    #[test]
    fn input_starts_at_request_start() {
        let (source, mut r, prev) = primed();
        r.write_batch(source.batch(Duration::from_millis(60) + D, Duration::from_millis(20)));

        let _third = run_and_dump(&mut r, &prev, "input_starts_at_request_start.wav");
        let _ = source;
    }

    /// Write [60.5ms, 80.5ms)+D — buffer starts 0.5ms *after* the request
    /// (sub-`SHIFT_THRESHOLD`). Analogous to
    /// `fresh::input_shifted_forward_within_threshold`.
    #[test]
    fn input_shifted_forward_within_threshold() {
        let (source, mut r, prev) = primed();
        let shift = Duration::from_micros(500);
        r.write_batch(source.batch(
            Duration::from_millis(60) + shift + D,
            Duration::from_millis(20),
        ));

        let _third = run_and_dump(
            &mut r,
            &prev,
            "input_shifted_forward_within_threshold.wav",
        );
        let _ = source;
    }

    /// Write [70ms, 90ms)+D — buffer overlaps only the back of the
    /// request. Analogous to `fresh::input_overlaps_request_end`.
    #[test]
    fn input_overlaps_request_end() {
        let (source, mut r, prev) = primed();
        r.write_batch(source.batch(Duration::from_millis(70) + D, Duration::from_millis(20)));

        let _third = run_and_dump(&mut r, &prev, "input_overlaps_request_end.wav");
        let _ = source;
    }

    /// Write [85ms, 100ms)+D — buffer entirely after the request.
    /// Analogous to `fresh::input_after_request`.
    #[test]
    fn input_after_request() {
        let (source, mut r, prev) = primed();
        r.write_batch(source.batch(Duration::from_millis(85) + D, Duration::from_millis(15)));

        let _third = run_and_dump(&mut r, &prev, "input_after_request.wav");
        let _ = source;
    }

    /// Two writes with a > 80ms gap between them: the first establishes
    /// the buffer non-empty, the second triggers the zero-fill branch in
    /// `write_batch`. No fresh analog.
    #[test]
    fn large_gap_append() {
        let (source, mut r, prev) = primed();
        r.write_batch(source.batch(Duration::from_millis(55) + D, Duration::from_millis(5)));
        r.write_batch(source.batch(Duration::from_millis(150) + D, Duration::from_millis(20)));

        let _third = run_and_dump(&mut r, &prev, "large_gap_append.wav");
        let _ = source;
    }
}
