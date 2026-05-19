use std::time::Duration;

use super::test_utils::*;
use super::*;

const RATE: u32 = 48_000;
const FIR_WINDOW: usize = 8;
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
    let samples = source.samples(D, Duration::from_millis(300) + D);
    dump_wav(&[&samples], RATE, "test_signal.wav");

    let source_5s = SignalSource::new(RATE, test_signal_5s());
    let samples_5s = source_5s.samples(D, Duration::from_millis(2000) + D);
    dump_wav(&[&samples_5s], RATE, "test_signal_5s.wav");
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D, Duration::from_millis(10)));
        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(10)));

        let samples =
            mono(r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(40));
        dump_wav(&[&pad, &samples], RATE, "fresh_input_before_request.wav");

        SignalAssertion {
            output: &samples[FIR_WINDOW..],
            source: &SignalSource::new(RATE, silence()),
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D, Duration::from_millis(10)));
        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_overlaps_request_start.wav",
        );

        SignalAssertion {
            output: &samples[FIR_WINDOW..(480 - FIR_WINDOW)],
            source: &source.shifted(out_start + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();
        SignalAssertion {
            output: &samples[(480 + FIR_WINDOW)..(960 - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(30) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &samples], RATE, "fresh_input_covers_request.wav");

        SignalAssertion {
            output: &samples[FIR_WINDOW..],
            source: &source.shifted(out_start + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(20) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(40) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_covers_request_grid_aligned.wav",
        );

        SignalAssertion {
            output: &samples[FIR_WINDOW..],
            source: &source.shifted(out_start + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(first_pts, Duration::from_millis(20)));
        r.write_batch(source.batch(
            Duration::from_millis(40) - shift + D,
            Duration::from_millis(20),
        ));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_shifted_backward_within_threshold.wav",
        );

        SignalAssertion {
            output: &samples[FIR_WINDOW..960],
            source: &source.shifted(out_start + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(Duration::from_millis(20) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(40) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_starts_at_request_start.wav",
        );

        SignalAssertion {
            output: &samples[FIR_WINDOW..],
            source: &source.shifted(out_start + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(first_pts, Duration::from_millis(20)));
        r.write_batch(source.batch(
            Duration::from_millis(40) + shift + D,
            Duration::from_millis(20),
        ));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_shifted_forward_within_threshold.wav",
        );

        // 500 us represents 24 samples
        SignalAssertion {
            output: &samples[0..(24 - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        SignalAssertion {
            output: &samples[(24 + FIR_WINDOW)..960],
            source: &source.shifted(first_pts + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
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
        let first_pts = Duration::from_millis(30) + D;
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(Duration::from_millis(30) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(20)));

        let out_start = Duration::from_millis(20) + D;
        let samples = mono(r.get_samples((out_start, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_overlaps_request_end.wav",
        );

        SignalAssertion {
            output: &samples[0..(480 - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        SignalAssertion {
            output: &samples[(480 + FIR_WINDOW)..960],
            source: &source.shifted(first_pts + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(Duration::from_millis(60) + D, Duration::from_millis(10)));
        r.write_batch(source.batch(Duration::from_millis(70) + D, Duration::from_millis(10)));

        let samples =
            mono(r.get_samples((Duration::from_millis(20) + D, Duration::from_millis(40) + D)));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &samples], RATE, "fresh_input_after_request.wav");

        SignalAssertion {
            output: &samples[FIR_WINDOW..],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
    }
}

/// Second `get_samples` call on a resampler that has already been driven
/// past the `before_first_resample` gate. The common setup writes two
/// 20ms batches ([10ms, 50ms)+D) and calls `get_samples((20ms, 40ms)+D)`,
/// leaving the resampler with ~10ms of buffered signal ([40ms,
/// 50ms)+D) split between `resampler_input_buffer` and `output_buffer`.
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();
        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(30) + D, Duration::from_millis(20)));
        let first =
            mono(r.get_samples((Duration::from_millis(20) + D, Duration::from_millis(40) + D)));
        (source, r, first)
    }

    /// No new data written after primed(). ~10ms of buffered signal
    /// [40ms, 50ms)+D remains, all before the [40ms, 60ms) request end.
    /// The resampler should produce the buffered signal followed by
    /// silence once the buffer runs out.
    #[test]
    fn no_new_input() {
        let (source, mut r, out_chunk_1) = primed();

        let out_chunk_2 =
            mono(r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)));
        assert_eq!(out_chunk_2.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &out_chunk_1, &out_chunk_2],
            RATE,
            "running_no_new_input.wav",
        );

        // ~10ms of buffered signal ≈ 480 samples at 48kHz
        let boundary = 480;
        SignalAssertion {
            output: &out_chunk_2[0..(boundary - FIR_WINDOW)],
            source: &source.shifted(Duration::from_millis(40) + D + SAMPLE48),
        }
        .assert();
        SignalAssertion {
            output: &out_chunk_2[(boundary + FIR_WINDOW)..(960 - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
    }

    /// Append a contiguous batch [50ms, 70ms)+D — buffer extends past the
    /// request end. Analogous to `fresh::input_covers_request`.
    #[test]
    fn input_covers_request() {
        let (source, mut r, out_chunk_1) = primed();
        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(20)));

        let out_chunk_2 =
            mono(r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)));
        assert_eq!(out_chunk_2.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &out_chunk_1, &out_chunk_2],
            RATE,
            "running_input_covers_request.wav",
        );

        SignalAssertion {
            output: &out_chunk_2,
            source: &source.shifted(Duration::from_millis(40) + D + SAMPLE48),
        }
        .assert();
    }

    // FLAG: input_covers_request_grid_aligned — the running-state buffer
    // has a fractional start (~40ms after init), so the
    // batch-grid-alignment property doesn't transfer.

    /// Append continuous signal [50ms, 70ms)+D with PTS shifted backward
    /// by 0.5ms (sub-`SHIFT_THRESHOLD`). Audio content is continuous with
    /// the previous input; only the timestamp overlaps the buffer end by
    /// 0.5ms. Analogous to `fresh::input_shifted_backward_within_threshold`.
    #[test]
    fn input_shifted_backward_within_threshold() {
        let (source, mut r, out_chunk_1) = primed();
        let shift = Duration::from_micros(500);
        let samples = source.samples(Duration::from_millis(50) + D, Duration::from_millis(70) + D);
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            Duration::from_millis(50) - shift + D,
            RATE,
        ));

        let out_chunk_2 =
            mono(r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)));
        assert_eq!(out_chunk_2.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &out_chunk_1, &out_chunk_2],
            RATE,
            "running_input_shifted_backward_within_threshold.wav",
        );

        SignalAssertion {
            output: &out_chunk_2,
            source: &source.shifted(Duration::from_millis(40) + D + SAMPLE48),
        }
        .assert();
    }

    // FLAG: input_starts_at_request_start — running buffer's start is
    // pinned at ~40+D ms; cannot be moved exactly to the request start.

    /// Append continuous signal [50ms, 70ms)+D with PTS shifted forward
    /// by 0.5ms (sub-`CONTINUITY_THRESHOLD`). Audio content is continuous
    /// with the previous input; only the timestamp has a 0.5ms gap.
    /// Analogous to `fresh::input_shifted_forward_within_threshold`.
    #[test]
    fn input_shifted_forward_within_threshold() {
        let (source, mut r, out_chunk_1) = primed();
        let shift = Duration::from_micros(500);
        let samples = source.samples(Duration::from_millis(50) + D, Duration::from_millis(70) + D);
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            Duration::from_millis(50) + shift + D,
            RATE,
        ));

        let out_chunk_2 =
            mono(r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)));
        assert_eq!(out_chunk_2.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &out_chunk_1, &out_chunk_2],
            RATE,
            "running_input_shifted_forward_within_threshold.wav",
        );

        SignalAssertion {
            output: &out_chunk_2,
            source: &source.shifted(Duration::from_millis(40) + D + SAMPLE48),
        }
        .assert();
    }

    /// Write 200ms of continuous signal [50ms, 250ms)+D with PTS shifted
    /// forward by 5ms (input appears 5ms late). The resampler should
    /// stretch the input over several chunks to fill the gap.
    #[test]
    fn drift_shift_forward_5ms() {
        let (source, mut r, out_chunk_1) = primed();
        let shift = Duration::from_millis(5);
        let samples = source.samples(
            Duration::from_millis(50) + D,
            Duration::from_millis(250) + D,
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            Duration::from_millis(50) + shift + D,
            RATE,
        ));

        let mut all_output = out_chunk_1;
        for i in 0..9 {
            let start = Duration::from_millis(40 + i * 20) + D;
            let end = start + Duration::from_millis(20);
            let chunk = mono(r.get_samples((start, end)));
            assert_eq!(chunk.len(), 960);
            all_output.extend_from_slice(&chunk);
        }
        let pad = silence_samples(RATE, Duration::from_millis(20));
        // should be aligned at 20-40ms range with test signal
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_shift_forward_5ms.wav",
        );

        let base_pts = Duration::from_millis(20) + D + SAMPLE48;

        // How much stretching already happened at mid and endpoint
        // For 5ms drift in 40ms (STRETCH_THRESHOLD) is 12.5%, so initial
        // stretch ratio should be 0.125 * 0.041 * 2 = 0.01025

        // Values bellow are set empirically based on previous test results,
        // they should match approximate values from comments

        SignalAssertion {
            output: &all_output[FIR_WINDOW..100],
            source: &source.shifted(base_pts + SAMPLE48 * FIR_WINDOW as u32),
        }
        .assert();
        // resampler is processing 256 samples at the time, so sample 960..1024 was
        // already processed before shift
        SignalAssertion {
            output: &all_output[1024..(1024 + 100)],
            source: &source
                .shifted(base_pts + SAMPLE48 * 1024 + Duration::from_secs_f64(0.0000012)),
        }
        .tolerance(0.01) // larger error because of ramping (changes rate quickly)
        .stretch(1.00281) // ramping happens here so initial stretching might be lower than expected
        .assert();

        // upper/lower bound calculation to sanity check values from tests:
        // offset: 20ms * 0.01025 = 205us
        // drift: 5ms-0.205ms = 4.795ms
        // stretch ratio: (4.795ms/40ms) * 0.041 * 2 = 0.00982975
        let batch_2_start = 960 * 2;
        SignalAssertion {
            output: &all_output[batch_2_start..(batch_2_start + 100)],
            source: &source.shifted(
                base_pts + Duration::from_secs_f64(batch_2_start as f64 / RATE as f64)
                    - Duration::from_secs_f64(0.0001666),
            ),
        }
        .tolerance(0.001)
        .stretch(1.01026) // not sure why it is lower (maybe still ramping)
        .assert();

        let batch_3_start = 960 * 3;
        SignalAssertion {
            output: &all_output[batch_3_start..(batch_3_start + 100)],
            source: &source.shifted(
                base_pts + Duration::from_secs_f64(batch_3_start as f64 / RATE as f64)
                    - Duration::from_secs_f64(0.0003703),
            ),
        }
        .tolerance(0.001)
        .stretch(1.010300)
        .assert();

        // upper/lower bound calculation to sanity check values from tests:
        // offset: 80ms * 0.01025 = 820us
        // drift: 5ms-0.82ms = 4.18ms
        // stretch ratio: (4.18ms/40ms) * 0.041 * 2 = 0.008569
        let batch_5_start = 960 * 5;
        SignalAssertion {
            output: &all_output[batch_5_start..(batch_5_start + 100)],
            source: &source.shifted(
                base_pts + Duration::from_secs_f64(batch_5_start as f64 / RATE as f64)
                    - Duration::from_secs_f64(0.0007778),
            ),
        }
        .tolerance(0.001)
        .stretch(1.010310)
        .assert();

        // upper/lower bound calculation to sanity check values from tests:
        // offset: 160ms * 0.01025 = 1640us
        // drift: 5ms-1.64ms = 3.36ms
        // stretch ratio: (3.36ms/40ms) * 0.041 * 2 = 0.006888 (this assume max initial stretch, so
        // real value will be higher)
        let batch_9_start = 960 * 9;
        SignalAssertion {
            output: &all_output[batch_9_start..(batch_9_start + 5)],
            source: &source.shifted(
                base_pts + Duration::from_secs_f64(batch_9_start as f64 / RATE as f64)
                    - Duration::from_secs_f64(0.0015929),
            ),
        }
        .tolerance(0.001)
        .stretch(1.009240)
        .assert();
    }

    /// Write 200ms of continuous signal [50ms, 250ms)+D with PTS shifted
    /// backward by 5ms (input appears 5ms early). The resampler should
    /// compress the input over several chunks to absorb the overlap.
    #[test]
    fn drift_shift_backward_5ms() {
        let (source, mut r, out_chunk_1) = primed();
        let shift = Duration::from_millis(5);
        let samples = source.samples(
            Duration::from_millis(50) + D,
            Duration::from_millis(250) + D,
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            Duration::from_millis(50) - shift + D,
            RATE,
        ));

        let mut all_output = out_chunk_1;
        for i in 0..9 {
            let start = Duration::from_millis(40 + i * 20) + D;
            let end = start + Duration::from_millis(20);
            let chunk = mono(r.get_samples((start, end)));
            assert_eq!(chunk.len(), 960);
            all_output.extend_from_slice(&chunk);
        }
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_shift_backward_5ms.wav",
        );

        let base_pts = Duration::from_millis(20) + D + SAMPLE48;

        // How much squashing already happened at mid and endpoint
        // For -5ms drift in 500ms (SQUASH_THRESHOLD) is 1%, so initial
        // squash ratio should be 0.01 * 0.041 * 2 = 0.00082 (ratio 0.99918)

        // Values below are set empirically based on previous test results,
        // they should match approximate values from comments

        SignalAssertion {
            output: &all_output[FIR_WINDOW..100],
            source: &source.shifted(base_pts + SAMPLE48 * FIR_WINDOW as u32),
        }
        .assert();
        // resampler is processing 256 samples at the time, so sample 960..1024 was
        // already processed before shift
        SignalAssertion {
            output: &all_output[1024..(1024 + 100)],
            source: &source
                .shifted(base_pts + SAMPLE48 * 1024 + Duration::from_secs_f64(0.0000003)),
        }
        .tolerance(0.01) // larger error because of ramping (changes rate faster)
        .stretch(0.99967) // ramping happens here so initial squashing might be lower than expected
        .assert();

        // upper/lower bound calculation to sanity check values from tests:
        // offset: 20ms * 0.00082 = 16.4us
        // drift: 5ms-0.0164ms = 4.984ms
        // squash ratio: (4.984ms/500ms) * 0.041 * 2 = 0.0008173
        let batch_2_start = 960 * 2;
        SignalAssertion {
            output: &all_output[batch_2_start..(batch_2_start + 100)],
            source: &source.shifted(
                base_pts
                    + Duration::from_secs_f64(batch_2_start as f64 / RATE as f64)
                    + Duration::from_secs_f64(0.0000135),
            ),
        }
        .tolerance(0.001)
        .stretch(0.999153) // still ramping up
        .assert();

        // batch 3
        let batch_3_start = 960 * 3;
        SignalAssertion {
            output: &all_output[batch_3_start..(batch_3_start + 100)],
            source: &source.shifted(
                base_pts
                    + Duration::from_secs_f64(batch_3_start as f64 / RATE as f64)
                    + Duration::from_secs_f64(0.0000299),
            ),
        }
        .tolerance(0.001)
        .stretch(0.9992)
        .assert();

        // upper/lower bound calculation to sanity check values from tests:
        // offset: 80ms * 0.00082 = 65.6us
        // drift: -5ms+0.0656ms = -4.934ms
        // squash ratio: (4.934ms/500ms) * 0.041 * 2 = 0.000808
        let batch_5_start = 960 * 5;
        SignalAssertion {
            output: &all_output[batch_5_start..(batch_5_start + 100)],
            source: &source.shifted(
                base_pts
                    + Duration::from_secs_f64(batch_5_start as f64 / RATE as f64)
                    + Duration::from_secs_f64(0.0000626),
            ),
        }
        .tolerance(0.001)
        .stretch(0.9992)
        .assert();

        // upper/lower bound calculation to sanity check values from tests:
        // offset: 160ms * 0.00082 = 131.2us
        // drift: 5ms-0.1312ms = -4.869ms
        // squash ratio: (4.869ms/500ms) * 0.041 * 2 = 0.000798 (this assumes max initial
        // squash, so real value will be higher)
        let batch_9_start = 960 * 9;
        SignalAssertion {
            output: &all_output[batch_9_start..(batch_9_start + 100)],
            source: &source.shifted(
                base_pts
                    + Duration::from_secs_f64(batch_9_start as f64 / RATE as f64)
                    + Duration::from_secs_f64(0.000128),
            ),
        }
        .tolerance(0.001)
        .stretch(0.99922)
        .assert();
    }

    /// Write 200ms of continuous signal [50ms, 250ms)+D, then call
    /// `get_samples` in 20ms chunks. Input is contiguous with primed()
    /// state — baseline for drift tests.
    #[test]
    fn drift_no_shift() {
        let (source, mut r, out_chunk_1) = primed();
        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(200)));

        let mut all_output = out_chunk_1;
        for i in 0..9 {
            let start = Duration::from_millis(40 + i * 20) + D;
            let end = start + Duration::from_millis(20);
            let chunk = mono(r.get_samples((start, end)));
            assert_eq!(chunk.len(), 960);
            all_output.extend_from_slice(&chunk);
        }
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &all_output], RATE, "running_drift_no_shift.wav");

        SignalAssertion {
            output: &all_output[FIR_WINDOW..],
            source: &source
                .shifted(Duration::from_millis(20) + D + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();
    }

    /// Write 200ms in two batches: first 100ms is offset by 5ms, second
    /// 100ms has no offset. The resampler should correct the initial drift
    /// and converge back to the no-drift baseline.
    #[test]
    fn drift_first_batch_offset_forward_5ms_second_no_offset() {
        let (source, mut r, out_chunk_1) = primed();
        let shift = Duration::from_millis(5);

        // First 100ms batch: PTS shifted forward by 5ms
        let samples_1 = source.samples(
            Duration::from_millis(50) + D,
            Duration::from_millis(150) + D,
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples_1),
            Duration::from_millis(50) + shift + D,
            RATE,
        ));
        // Second 100ms batch: no offset (contiguous with first batch's real data)
        r.write_batch(source.batch(Duration::from_millis(150) + D, Duration::from_millis(100)));

        let mut all_output = out_chunk_1;
        for i in 0..9 {
            let start = Duration::from_millis(40 + i * 20) + D;
            let end = start + Duration::from_millis(20);
            let chunk = mono(r.get_samples((start, end)));
            assert_eq!(chunk.len(), 960);
            all_output.extend_from_slice(&chunk);
        }
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_first_batch_offset_forward_5ms_second_no_offset.wav",
        );

        SignalAssertion {
            output: &all_output[FIR_WINDOW..],
            source: &source
                .shifted(Duration::from_millis(20) + D + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();
    }

    /// Write 200ms in two batches: first 100ms is offset backward by 5ms,
    /// second 100ms has no offset. The resampler should correct the initial
    /// drift and converge back to the no-drift baseline.
    #[test]
    fn drift_first_batch_offset_backward_5ms_second_no_offset() {
        let (source, mut r, out_chunk_1) = primed();
        let shift = Duration::from_millis(5);

        // First 100ms batch: PTS shifted backward by 5ms
        let samples_1 = source.samples(
            Duration::from_millis(50) + D,
            Duration::from_millis(150) + D,
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples_1),
            Duration::from_millis(50) - shift + D,
            RATE,
        ));
        // Second 100ms batch: no offset (contiguous with first batch's real data)
        r.write_batch(source.batch(Duration::from_millis(150) + D, Duration::from_millis(100)));

        let mut all_output = out_chunk_1;
        for i in 0..9 {
            let start = Duration::from_millis(40 + i * 20) + D;
            let end = start + Duration::from_millis(20);
            let chunk = mono(r.get_samples((start, end)));
            assert_eq!(chunk.len(), 960);
            all_output.extend_from_slice(&chunk);
        }
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_first_batch_offset_backward_5ms_second_no_offset.wav",
        );

        SignalAssertion {
            output: &all_output[FIR_WINDOW..],
            source: &source
                .shifted(Duration::from_millis(20) + D + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();
    }

    /// Write [100ms, 120ms)+D after primed (buffered end at 50ms). The 50ms
    /// gap exceeds `STRETCH_THRESHOLD` so gap-fill prepends zeros. Output:
    /// - [40, 90)+D  = silence (gap-fill zeros)
    /// - [90, 100)+D = primed leftover (source [40, 50)+D)
    /// - [100, 120)+D = written batch
    #[test]
    fn drift_shift_forward_50ms() {
        let (source, mut r, out_chunk_1) = primed();
        r.write_batch(source.batch(Duration::from_millis(100) + D, Duration::from_millis(20)));

        let mut all_output = out_chunk_1;
        for i in 0..4 {
            let start = Duration::from_millis(40 + i * 20) + D;
            let end = start + Duration::from_millis(20);
            let chunk = mono(r.get_samples((start, end)));
            assert_eq!(chunk.len(), 960);
            all_output.extend_from_slice(&chunk);
        }
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_shift_forward_50ms.wav",
        );

        // 480 - There is still 10ms unused from prime writes
        // 64 - resampler is processing 256 at a time, so after prime there are
        //   still 64 samples left
        let silence_start = 960 + 64;
        let silence_end = silence_start + 960 * 2 + 480;
        SignalAssertion {
            output: &all_output[(silence_start + FIR_WINDOW)..(silence_end - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        let prime_batch_start = 960 * 3 + 480 + 64;
        SignalAssertion {
            output: &all_output
                [(prime_batch_start + FIR_WINDOW)..(prime_batch_start + (480 - 64) - FIR_WINDOW)],
            source: &source
                .shifted(Duration::from_millis(40) + D + SAMPLE48 * (FIR_WINDOW as u32 + 64)),
        }
        .assert();
        let batch_start = 960 * 4;
        SignalAssertion {
            output: &all_output[(batch_start + FIR_WINDOW)..(batch_start + 960 - FIR_WINDOW)],
            source: &source.shifted(Duration::from_millis(100) + D + SAMPLE48 * FIR_WINDOW as u32),
        }
        .assert();
    }

    /// Same setup as `primed()` but with PTS shifted up by 1000ms to leave
    /// room for backward drift. Writes [1010, 1050)+D, reads [1020, 1040)+D.
    /// Then writes 12 contiguous 100ms batches of audio from [1050, 2250)+D,
    /// each with PTS shifted backward by `(i+1) * 50ms`. Each batch is 100ms
    /// of audio but its PTS only advances by 50ms, so `input_buffer_end_pts`
    /// falls behind by 50ms per batch. Each batch's PTS stays within the
    /// 80ms anti-overlap tolerance of the previous `input_buffer_end_pts`.
    ///
    /// After all writes the accumulated drift is ~600ms. Read at
    /// [1040, 1060)+D → triggers DROP.
    #[test]
    fn drift_shift_backward_600ms() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal_5s());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();
        r.write_batch(source.batch(Duration::from_millis(1010) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(1030) + D, Duration::from_millis(20)));
        let out_chunk_1 = mono(r.get_samples((
            Duration::from_millis(1020) + D,
            Duration::from_millis(1040) + D,
        )));

        // Each 100ms batch has PTS shifted backward by (i+1)*50ms — would
        // require squashing by 50% to handle without drops.
        for i in 0..12u64 {
            let content_start = Duration::from_millis(1050 + i * 100) + D;
            let content_end = Duration::from_millis(1150 + i * 100) + D;
            let samples = source.samples(content_start, content_end);
            r.write_batch(InputAudioSamples::new(
                AudioSamples::Mono(samples),
                content_start - Duration::from_millis((i + 1) * 50),
                RATE,
            ));
        }
        // Each batch introduces 50ms of backward drift; after 12 batches the
        // total is 600ms — past SQUASH_THRESHOLD, triggering DROP.

        let chunk = mono(r.get_samples((
            Duration::from_millis(1040) + D,
            Duration::from_millis(1060) + D,
        )));
        assert_eq!(chunk.len(), 960);
        let mut all_output = out_chunk_1;
        all_output.extend_from_slice(&chunk);
        let pad = silence_samples(RATE, Duration::from_millis(1020));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_shift_backward_600ms.wav",
        );

        SignalAssertion {
            output: &all_output[FIR_WINDOW..960],
            source: &source
                .shifted(Duration::from_millis(1020) + D + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();

        // After DROP, the buffer is realigned. The first 64 samples are
        // leftover from the primed run's output_buffer. Content is shifted
        // by 600ms relative to the normal timeline.
        SignalAssertion {
            output: &chunk[(64 + FIR_WINDOW)..],
            source: &source.shifted(
                Duration::from_millis(1040)
                    + D
                    + SAMPLE48 * (64 + FIR_WINDOW as u32 - 1)
                    + Duration::from_millis(600),
            ),
        }
        .assert();
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
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();
        r.write_batch(source.batch(Duration::from_millis(10) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(30) + D, Duration::from_millis(20)));
        let first =
            mono(r.get_samples((Duration::from_millis(20) + D, Duration::from_millis(40) + D)));
        let second =
            mono(r.get_samples((Duration::from_millis(40) + D, Duration::from_millis(60) + D)));
        let mut prev = Vec::with_capacity(first.len() + second.len());
        prev.extend_from_slice(&first);
        prev.extend_from_slice(&second);
        (source, r, prev)
    }

    /// Assert primed() output. Input was [10, 50)+D, reads were [20, 60)+D.
    /// First 20ms of output ([20, 40)+D) should reproduce the source.
    /// Second 20ms ([40, 60)+D) has only 10ms of input — first half is
    /// signal, second half is silence.
    #[test]
    fn primed_output() {
        let (source, _r, all_output) = primed();

        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &all_output], RATE, "drained_primed_output.wav");

        // [20, 40)+D — fully covered by input
        SignalAssertion {
            output: &all_output[FIR_WINDOW..960],
            source: &source
                .shifted(Duration::from_millis(20) + D + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();
        // [40, 50)+D — last 10ms of input
        SignalAssertion {
            output: &all_output[960..(960 + 470)],
            source: &source.shifted(Duration::from_millis(20) + D + SAMPLE48 * 961),
        }
        .assert();
        // [50, 60)+D — no input, should be silence
        SignalAssertion {
            output: &all_output[(960 + 490)..1910],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
    }

    /// Read [60, 80)+D on already drained state — no new input written.
    /// Output should be silence.
    #[test]
    fn no_new_input() {
        let (_source, mut r, _prev) = primed();

        let chunk =
            mono(r.get_samples((Duration::from_millis(60) + D, Duration::from_millis(80) + D)));
        assert_eq!(chunk.len(), 960);

        SignalAssertion {
            output: &chunk,
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
    }

    /// Write [50, 70)+D and [70, 90)+D after drained state, then read
    /// [60, 80)+D. Input fully covers the request. Output should
    /// reproduce the source.
    #[test]
    fn input_covers_request() {
        let (source, mut r, mut all_output) = primed();

        r.write_batch(source.batch(Duration::from_millis(50) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(70) + D, Duration::from_millis(20)));

        let chunk =
            mono(r.get_samples((Duration::from_millis(60) + D, Duration::from_millis(80) + D)));
        assert_eq!(chunk.len(), 960);
        all_output.extend_from_slice(&chunk);

        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "drained_input_covers_request.wav",
        );

        SignalAssertion {
            output: &chunk[FIR_WINDOW..],
            source: &source
                .shifted(Duration::from_millis(60) + D + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();
    }

    /// Write [55, 75)+D and [75, 95)+D after drained state, then read
    /// [60, 80)+D. 5ms gap from drained end (50ms) to new input (55ms),
    /// but input fully covers the request. Output should reproduce the
    /// source.
    #[test]
    fn input_covers_request_5ms_gap() {
        let (source, mut r, mut all_output) = primed();

        r.write_batch(source.batch(Duration::from_millis(55) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(75) + D, Duration::from_millis(20)));

        let chunk =
            mono(r.get_samples((Duration::from_millis(60) + D, Duration::from_millis(80) + D)));
        assert_eq!(chunk.len(), 960);
        all_output.extend_from_slice(&chunk);

        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "drained_input_covers_request_5ms_gap.wav",
        );

        SignalAssertion {
            output: &chunk[FIR_WINDOW..],
            source: &source
                .shifted(Duration::from_millis(60) + D + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();
    }

    /// Write [65, 85)+D and [85, 105)+D after drained state, then read
    /// [60, 80)+D. 15ms gap from drained end (50ms) to new input (65ms).
    /// Input starts 5ms into the request — first 5ms of output is
    /// silence, remainder reproduces the source.
    #[test]
    fn input_covers_request_15ms_gap() {
        let (source, mut r, mut all_output) = primed();

        r.write_batch(source.batch(Duration::from_millis(65) + D, Duration::from_millis(20)));
        r.write_batch(source.batch(Duration::from_millis(85) + D, Duration::from_millis(20)));

        let chunk =
            mono(r.get_samples((Duration::from_millis(60) + D, Duration::from_millis(80) + D)));
        assert_eq!(chunk.len(), 960);
        all_output.extend_from_slice(&chunk);

        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "drained_input_covers_request_15ms_gap.wav",
        );

        // 5ms at 48kHz = 240 samples of silence before input starts
        let boundary = 240;
        SignalAssertion {
            output: &chunk[0..(boundary - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        SignalAssertion {
            output: &chunk[(boundary + FIR_WINDOW)..960],
            source: &source
                .shifted(Duration::from_millis(65) + D + SAMPLE48 * (FIR_WINDOW as u32 + 1)),
        }
        .assert();
    }
}
