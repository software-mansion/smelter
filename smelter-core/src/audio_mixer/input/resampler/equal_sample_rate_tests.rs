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
        dump_wav(&samples, RATE, "input_before_request.wav");

        SignalAssertion {
            output: &samples,
            output_window: 0..samples.len(),
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
        dump_wav(&samples, RATE, "input_overlaps_request_start.wav");

        // Skip the trailing 10 samples of the audio portion: as the FIR
        // window slides across the audio→silence boundary it averages in
        // padded zeros, so the last few samples decay toward silence.
        SignalAssertion {
            output: &samples,
            output_window: 0..470,
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48,
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
        dump_wav(&samples, RATE, "input_covers_request.wav");

        SignalAssertion {
            output: &samples,
            output_window: 0..samples.len(),
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48,
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
        dump_wav(&samples, RATE, "input_covers_request_grid_aligned.wav");

        SignalAssertion {
            output: &samples,
            output_window: 0..samples.len(),
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48,
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
    /// Output should reproduce the source with no boundary effects.
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
        dump_wav(&samples, RATE, "input_shifted_backward_within_threshold.wav");

        SignalAssertion {
            output: &samples,
            output_window: 0..samples.len(),
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48,
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
        dump_wav(&samples, RATE, "input_starts_at_request_start.wav");

        SignalAssertion {
            output: &samples,
            output_window: 0..samples.len(),
            source: &source,
            source_pts_at_window_start: out_start + SAMPLE48,
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
        dump_wav(&samples, RATE, "input_shifted_forward_within_threshold.wav");

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
        dump_wav(&samples, RATE, "input_overlaps_request_end.wav");

        // Skip 10 samples on either side of the silence→audio boundary
        // (and at the very front): the leading edge has FIR warmup
        // settling, and the trailing edge has sinc ringing aliased over
        // from the real input across the boundary. Neither shows up more
        // than a handful of samples in.
        SignalAssertion {
            output: &samples,
            output_window: 10..470,
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
        dump_wav(&samples, RATE, "input_after_request.wav");

        SignalAssertion {
            output: &samples,
            output_window: 0..samples.len(),
            source: &SignalSource::new(RATE, silence()),
            source_pts_at_window_start: Duration::ZERO,
            tolerance: 1e-3,
        }
        .assert();
    }
}
