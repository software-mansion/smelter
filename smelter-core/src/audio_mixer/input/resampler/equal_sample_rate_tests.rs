use std::time::Duration;

use super::test_utils::*;
use super::*;

const RATE: u32 = 48_000;
const FIR_WINDOW: usize = 8;
const SAMPLE48: Duration = Duration::from_nanos(1_000_000_000 / 48_000);

/// Frames faded in after every resync, `CROSSFADE_DURATION` at `RATE`.
const FADE_IN: usize = 240;

fn mono(samples: AudioSamples) -> Vec<f64> {
    let AudioSamples::Mono(s) = samples else {
        panic!("expected Mono output");
    };
    s
}

/// Read `count` consecutive 20ms chunks starting at `start` and concatenate them.
fn read_chunks(r: &mut InputResampler, start: Timestamp, count: u32) -> Vec<f64> {
    let mut output = Vec::new();
    for i in 0..count {
        let chunk_start = start + Duration::from_millis(20) * i;
        let chunk = mono(r.get_samples((chunk_start, chunk_start + Duration::from_millis(20))));
        assert_eq!(chunk.len(), 960);
        output.extend_from_slice(&chunk);
    }
    output
}

/// Assert that drift control corrects `drift` (positive if input is placed later than the output
/// timeline) like the proportional controller, with time constant `tau`: after `t` of correction
/// the output is `drift * (1 - e^(-t / tau))` behind its original alignment. Measured on a 1ms
/// window in every 20ms chunk after correction starts at output frame `start`. The ratio ramps
/// in, so early on the output lags the model; allowed error is 20% of the expected offset plus
/// 30µs.
///
/// `output[i]` is aligned with `source` at `base_pts + (i + 1) / RATE` before the drift.
fn assert_drift_converges(
    output: &[f64],
    source: &SignalSource,
    base_pts: Timestamp,
    start: usize,
    drift: Duration,
    tau: Duration,
    is_stretch: bool,
) {
    let sign = match is_stretch {
        true => -1.0,
        false => 1.0,
    };
    let mut window = (start / 960 + 1) * 960 + 300;
    while window + 48 <= output.len() {
        let elapsed = (window + 24 - start) as f64 / RATE as f64;
        let expected = sign * drift.as_secs_f64() * (1.0 - f64::exp(-elapsed / tau.as_secs_f64()));
        let tolerance = expected.abs() * 0.2 + 30e-6;
        // `SAMPLE48` is truncated to whole nanoseconds, too imprecise to multiply by a frame
        // index this large.
        let reference = source.shifted(
            base_pts + Timestamp::from_secs_f64((window + 1) as f64 / RATE as f64 + expected),
        );
        let (offset, rms) = measure_offset(
            &output[window..(window + 48)],
            &reference,
            Duration::from_secs_f64(tolerance),
        );
        assert!(
            rms < 0.05 && offset.abs() < tolerance,
            "window @ {window}: expected offset {:.1}us ± {:.1}us, measured {:.1}us (rms {rms:.4})",
            expected * 1e6,
            tolerance * 1e6,
            (expected + offset) * 1e6,
        );
        window += 960;
    }
}

/// Not a real test — just dumps 5 seconds of the default `test_signal()`
/// to a WAV file so its waveform can be inspected outside the test runner.
/// Always passes.
#[test]
fn dump_test_signal() {
    let source = SignalSource::new(RATE, test_signal());
    let samples = source.samples(D, D + Duration::from_millis(300));
    dump_wav(&[&samples], RATE, "test_signal.wav");

    let source_5s = SignalSource::new(RATE, test_signal_5s());
    let samples_5s = source_5s.samples(D, D + Duration::from_millis(2000));
    dump_wav(&[&samples_5s], RATE, "test_signal_5s.wav");
}

/// First `get_samples` call on a freshly-constructed resampler — the
/// resync gate is still armed, so every test in this module exercises one
/// branch of `try_resync_after_discontinuity`. Tests are ordered by the
/// position of the buffered input relative to the request window: way
/// before → straddling start → covering → straddling end → way after.
///
/// Unless a test is about a short buffer, input reaches ~80ms past the
/// request end, like the queue delivers it. The gate only starts once real
/// input reaches one chunk plus `RESYNC_LEAD` past the request end.
///
/// All PTS values are perturbed by [`D`] so we don't accidentally rely on
/// round-millisecond timestamps.
///
/// The first `FADE_IN` frames after the resync are faded in. Assertions on
/// the plain signal start after them, plus `FIR_WINDOW` frames for the FIR
/// transient at the start of the filter.
mod fresh {
    use super::*;

    /// Input [0, 20ms), request [40ms, 60ms). Input entirely before the
    /// request window — the gate drops it; output is silence.
    #[test]
    fn input_before_request() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D, Duration::from_millis(10)));
        r.write_batch(source.batch(D + Duration::from_millis(10), Duration::from_millis(10)));

        let samples =
            mono(r.get_samples((D + Duration::from_millis(40), D + Duration::from_millis(60))));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(40));
        dump_wav(&[&pad, &samples], RATE, "fresh_input_before_request.wav");

        SignalAssertion {
            output: &samples,
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
    }

    /// Input [0, 30ms), request [20ms, 40ms). Input ends inside the
    /// request, far from the lead the gate needs, so the gate waits and
    /// the output is silence. By the next request [40ms, 60ms) the input
    /// is entirely in the past and gets dropped, it never plays.
    #[test]
    fn input_ends_within_request() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D, Duration::from_millis(10)));
        r.write_batch(source.batch(D + Duration::from_millis(10), Duration::from_millis(20)));

        let samples = read_chunks(&mut r, D + Duration::from_millis(20), 2);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_ends_within_request.wav",
        );

        SignalAssertion {
            output: &samples,
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        assert!(r.needs_input_resync);
        assert_eq!(r.resampler_input_buffer.frames(), 0);
    }

    /// Input [10ms, 130ms), request [20ms, 40ms). Input fully covers the
    /// request — the gate drains the [10ms, 20ms) prefix and fades in the
    /// new front; subsequent resample iterations sit in the on-time
    /// dead-band. Output should reproduce the source at the requested PTS.
    #[test]
    fn input_covers_request() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D + Duration::from_millis(10), Duration::from_millis(60)));
        r.write_batch(source.batch(D + Duration::from_millis(70), Duration::from_millis(60)));

        let out_start = D + Duration::from_millis(20);
        let samples = mono(r.get_samples((out_start, D + Duration::from_millis(40))));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &samples], RATE, "fresh_input_covers_request.wav");

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &source.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Same as [`input_covers_request`] but with the input batches aligned
    /// exactly to the request grid: [0, 20ms), [20ms, 40ms), …, [100ms,
    /// 120ms). The drain stops on a clean batch boundary instead of
    /// mid-batch.
    #[test]
    fn input_covers_request_grid_aligned() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        for i in 0..6 {
            r.write_batch(
                source.batch(D + Duration::from_millis(20) * i, Duration::from_millis(20)),
            );
        }

        let out_start = D + Duration::from_millis(20);
        let samples = mono(r.get_samples((out_start, D + Duration::from_millis(40))));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_covers_request_grid_aligned.wav",
        );

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &source.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Same as [`input_starts_at_request_start`] but the input is shifted
    /// **backward by 0.5ms** (still well below `SHIFT_THRESHOLD = 2ms`).
    /// `try_resync_after_discontinuity` drains 24 too-old samples from
    /// the front of the buffer, restoring alignment; the main loop
    /// stays in the on-time dead-band.
    #[test]
    fn input_shifted_backward_within_threshold() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let shift = Duration::from_micros(500);
        let first_pts = D + Duration::from_millis(20) - shift;
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(first_pts, Duration::from_millis(50)));
        r.write_batch(source.batch(
            D + Duration::from_millis(70) - shift,
            Duration::from_millis(50),
        ));

        let out_start = D + Duration::from_millis(20);
        let samples = mono(r.get_samples((out_start, D + Duration::from_millis(40))));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_shifted_backward_within_threshold.wav",
        );

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &source.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Input starts exactly at request start: input [20ms, 120ms),
    /// request [20ms, 40ms). `input_buffer_start_pts == pts_range.0`, so
    /// neither the drain nor the pad branch in
    /// `try_resync_after_discontinuity` fires. The first `FADE_IN` frames
    /// are the faded-in source, the rest is the plain source.
    #[test]
    fn input_starts_at_request_start() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D + Duration::from_millis(20), Duration::from_millis(50)));
        r.write_batch(source.batch(D + Duration::from_millis(70), Duration::from_millis(50)));

        let out_start = D + Duration::from_millis(20);
        let samples = mono(r.get_samples((out_start, D + Duration::from_millis(40))));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "fresh_input_starts_at_request_start.wav",
        );

        // The FIR transient at the start is negligible, the fade-in gain is ~0 there.
        SignalAssertion {
            output: &samples[..FADE_IN],
            source: &source.shifted(out_start + SAMPLE48).faded_in(FADE_IN),
        }
        .assert();
        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &source.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Same as [`input_starts_at_request_start`] but the input is shifted
    /// **forward by 0.5ms** (still well below `SHIFT_THRESHOLD = 2ms`).
    /// `try_resync_after_discontinuity` pads 24 silent samples at the
    /// front of the buffer to align the timeline; the main loop stays
    /// in the on-time dead-band (no stretch/squash applied).
    ///
    /// Output: 24 silent samples followed by the faded-in source signal.
    #[test]
    fn input_shifted_forward_within_threshold() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let shift = Duration::from_micros(500);
        let first_pts = D + Duration::from_millis(20) + shift;
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(first_pts, Duration::from_millis(50)));
        r.write_batch(source.batch(
            D + Duration::from_millis(70) + shift,
            Duration::from_millis(50),
        ));

        let out_start = D + Duration::from_millis(20);
        let samples = mono(r.get_samples((out_start, D + Duration::from_millis(40))));
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
        let start = 24 + FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &source.shifted(first_pts + SAMPLE48 * (start as u32 - 24 + 1)),
        }
        .assert();
    }

    /// Input [30ms, 110ms), request [20ms, 40ms). Input overlaps only the
    /// end of the request — `try_resync_after_discontinuity` pads the
    /// front of the buffer with silence so the timeline lines up.
    ///
    /// Ideal output:
    /// - output[0..480]   = silence
    /// - output[480..960] = audio at input [30ms, 40ms), faded in
    #[test]
    fn input_overlaps_request_end() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let first_pts = D + Duration::from_millis(30);
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(first_pts, Duration::from_millis(40)));
        r.write_batch(source.batch(D + Duration::from_millis(70), Duration::from_millis(40)));

        let out_start = D + Duration::from_millis(20);
        let samples = mono(r.get_samples((out_start, D + Duration::from_millis(40))));
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
        let start = 480 + FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..960],
            source: &source.shifted(first_pts + SAMPLE48 * (start as u32 - 480 + 1)),
        }
        .assert();
    }

    /// Input [60ms, 80ms), request [20ms, 40ms). Input entirely after the
    /// request but already far enough ahead for the gate, which pads 40ms
    /// of silence in front of it. Output is silence.
    #[test]
    fn input_after_request() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D + Duration::from_millis(60), Duration::from_millis(10)));
        r.write_batch(source.batch(D + Duration::from_millis(70), Duration::from_millis(10)));

        let samples =
            mono(r.get_samples((D + Duration::from_millis(20), D + Duration::from_millis(40))));
        assert_eq!(samples.len(), 960);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &samples], RATE, "fresh_input_after_request.wav");

        SignalAssertion {
            output: &samples,
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        assert!(!r.needs_input_resync);
    }

    /// The gate starts only once real input reaches one chunk plus
    /// `RESYNC_LEAD` past the request end. Two resamplers get input
    /// starting at the request start, one ending 1ms short of that
    /// threshold and one 1ms past it.
    #[test]
    fn gate_waits_for_lead() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let out_start = D + Duration::from_millis(20);
        let out_end = D + Duration::from_millis(40);

        let mut short = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();
        let threshold = out_end + short.resampler.input_frame_duration() + RESYNC_LEAD;
        let short_len = threshold - out_start - Timestamp::from_millis(1);
        short.write_batch(source.batch(out_start, short_len.to_duration_saturating()));
        let samples = mono(short.get_samples((out_start, out_end)));
        SignalAssertion {
            output: &samples,
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        assert!(short.needs_input_resync);

        let mut long = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();
        let long_len = threshold - out_start + Timestamp::from_millis(1);
        long.write_batch(source.batch(out_start, long_len.to_duration_saturating()));
        let samples = mono(long.get_samples((out_start, out_end)));
        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &source.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
        assert!(!long.needs_input_resync);
    }

    /// The gate keeps short input instead of dropping it, so input that
    /// keeps arriving contiguously starts as soon as it is far enough
    /// ahead. Input [20ms, 50ms) is too short for request [20ms, 40ms);
    /// after [50ms, 110ms) arrives, request [40ms, 60ms) drains the input
    /// up to 40ms and plays it faded in.
    #[test]
    fn gate_keeps_short_input() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D + Duration::from_millis(20), Duration::from_millis(30)));
        let first =
            mono(r.get_samples((D + Duration::from_millis(20), D + Duration::from_millis(40))));
        r.write_batch(source.batch(D + Duration::from_millis(50), Duration::from_millis(60)));
        let out_start = D + Duration::from_millis(40);
        let second = mono(r.get_samples((out_start, D + Duration::from_millis(60))));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &first, &second],
            RATE,
            "fresh_gate_keeps_short_input.wav",
        );

        SignalAssertion {
            output: &first,
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &second[start..],
            source: &source.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }
}

/// Batches written while the resampler is resyncing, before the gate
/// places the buffered input. `write_batch` keeps the buffered input on
/// its timeline: gaps are padded, an overlap of at least `SEAM_THRESHOLD`
/// starts a new timeline that replaces it.
mod resync {
    use super::*;

    /// Input [20ms, 40ms), then [55ms, 140ms) — a 15ms gap written before
    /// the first request. The gap is padded inside the buffer:
    /// concealment of [20ms, 40ms) for 5ms, silence until 55ms, and the
    /// second batch faded in at its PTS.
    #[test]
    fn gap_is_padded() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D + Duration::from_millis(20), Duration::from_millis(20)));
        r.write_batch(source.batch(D + Duration::from_millis(55), Duration::from_millis(85)));

        let out_start = D + Duration::from_millis(20);
        let samples = read_chunks(&mut r, out_start, 3);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &samples], RATE, "resync_gap_is_padded.wav");

        // [20ms, 40ms) — first batch, faded in by the gate
        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..(960 - FIR_WINDOW)],
            source: &source.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
        // [45ms, 55ms) — after 5ms of concealment, silence until the second batch
        SignalAssertion {
            output: &samples[(1200 + FIR_WINDOW)..(1680 - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        // [55ms, 80ms) — second batch, faded in by `write_batch`
        let start = 1680 + FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &source.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Input [20ms, 140ms), then a batch at [40ms, 160ms) with different
    /// content. It overlaps the buffered input by 100ms, at least
    /// `SEAM_THRESHOLD`, so it starts a new timeline: the buffered input is
    /// discarded and the gate pads silence until the batch, faded in at
    /// 40ms.
    #[test]
    fn large_overlap_replaces_buffered_input() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let other = SignalSource::new(RATE, |_| 0.5);
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D + Duration::from_millis(20), Duration::from_millis(120)));
        r.write_batch(other.batch(D + Duration::from_millis(40), Duration::from_millis(120)));

        let out_start = D + Duration::from_millis(20);
        let samples = read_chunks(&mut r, out_start, 3);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "resync_large_overlap_replaces_buffered_input.wav",
        );

        // [20ms, 40ms) — silence before the batch
        SignalAssertion {
            output: &samples[..(960 - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        // [40ms, 80ms) — the batch, faded in by the gate
        let start = 960 + FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &other.shifted(out_start + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Input [20ms, 100ms), then source content [100ms, 180ms) with PTS
    /// 5ms early (95ms). The 5ms overlap is under `SEAM_THRESHOLD`, so the
    /// batch is appended as is. The buffered input's start is computed back
    /// from its end, so everything before the overlap is placed 5ms early:
    /// request [20ms, 40ms) plays the source from 25ms.
    #[test]
    fn small_overlap_places_earlier_input_early() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();

        r.write_batch(source.batch(D + Duration::from_millis(20), Duration::from_millis(80)));
        let samples = source.samples(
            D + Duration::from_millis(100),
            D + Duration::from_millis(180),
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            D + Duration::from_millis(95),
            RATE,
        ));

        let out_start = D + Duration::from_millis(20);
        let samples = read_chunks(&mut r, out_start, 3);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &samples],
            RATE,
            "resync_small_overlap_places_earlier_input_early.wav",
        );

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &samples[start..],
            source: &source
                .shifted(out_start + Duration::from_millis(5) + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }
}

/// Second `get_samples` call on a resampler that has already been driven
/// past the resync gate. The common setup writes [10ms, 130ms)+D and calls
/// `get_samples((20ms, 40ms)+D)`, leaving the resampler with ~90ms of
/// buffered signal ([40ms, 130ms)+D) split between `resampler_input_buffer`
/// and `output_buffer`.
///
/// Each test then writes new input after the buffered data and keeps
/// calling `get_samples` from (40ms, 60ms)+D on. The concatenated output
/// of all `get_samples` calls is dumped to a WAV file for inspection.
mod running {
    use super::*;

    /// Common init: [10, 130)+D worth of input, then `get_samples((20, 40)+D)`.
    fn primed() -> (SignalSource, InputResampler, Vec<f64>) {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();
        r.write_batch(source.batch(D + Duration::from_millis(10), Duration::from_millis(60)));
        r.write_batch(source.batch(D + Duration::from_millis(70), Duration::from_millis(60)));
        let first =
            mono(r.get_samples((D + Duration::from_millis(20), D + Duration::from_millis(40))));
        (source, r, first)
    }

    /// No new data written after primed(). The buffered input runs out
    /// at 130ms: it is played to its end, followed by 5ms of concealment
    /// and silence.
    #[test]
    fn input_runs_out() {
        let (source, mut r, mut all_output) = primed();

        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 6));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &all_output], RATE, "running_input_runs_out.wav");

        // [20ms, 130ms) — the whole input
        let start = FADE_IN + FIR_WINDOW;
        let input_end = 960 * 5 + 480;
        SignalAssertion {
            output: &all_output[start..(input_end - FIR_WINDOW)],
            source: &source.shifted(D + Duration::from_millis(20) + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
        // [135ms, 160ms) — silence after the concealment
        SignalAssertion {
            output: &all_output[(input_end + 240 + FIR_WINDOW)..],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        assert!(r.needs_input_resync);
    }

    /// Append a contiguous batch [130ms, 150ms)+D. Output should reproduce
    /// the source across the batch boundary.
    #[test]
    fn input_covers_request() {
        let (source, mut r, mut all_output) = primed();
        r.write_batch(source.batch(D + Duration::from_millis(130), Duration::from_millis(20)));

        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 5));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_input_covers_request.wav",
        );

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..],
            source: &source.shifted(D + Duration::from_millis(20) + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Append continuous signal [130ms, 150ms)+D with PTS shifted backward
    /// by 0.5ms (sub-`SHIFT_THRESHOLD`). Audio content is continuous with
    /// the previous input; only the timestamp overlaps the buffer end by
    /// 0.5ms. Analogous to `fresh::input_shifted_backward_within_threshold`.
    #[test]
    fn input_shifted_backward_within_threshold() {
        let (source, mut r, mut all_output) = primed();
        let shift = Duration::from_micros(500);
        let samples = source.samples(
            D + Duration::from_millis(130),
            D + Duration::from_millis(150),
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            D + Duration::from_millis(130) - shift,
            RATE,
        ));

        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 5));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_input_shifted_backward_within_threshold.wav",
        );

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..],
            source: &source.shifted(D + Duration::from_millis(20) + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Append continuous signal [130ms, 150ms)+D with PTS shifted forward
    /// by 0.5ms (sub-`SEAM_THRESHOLD`). Audio content is continuous
    /// with the previous input; only the timestamp has a 0.5ms gap.
    /// Analogous to `fresh::input_shifted_forward_within_threshold`.
    #[test]
    fn input_shifted_forward_within_threshold() {
        let (source, mut r, mut all_output) = primed();
        let shift = Duration::from_micros(500);
        let samples = source.samples(
            D + Duration::from_millis(130),
            D + Duration::from_millis(150),
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            D + Duration::from_millis(130) + shift,
            RATE,
        ));

        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 5));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_input_shifted_forward_within_threshold.wav",
        );

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..],
            source: &source.shifted(D + Duration::from_millis(20) + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Write 200ms of continuous signal [130ms, 330ms)+D with PTS shifted
    /// forward by 5ms (input appears 5ms late). The gap is below
    /// `SEAM_THRESHOLD`, so it is appended and the whole buffered input
    /// is placed 5ms later. The resampler should stretch the input over
    /// several chunks to fill the gap.
    #[test]
    fn drift_shift_forward_5ms() {
        let (source, mut r, out_chunk_1) = primed();
        let shift = Duration::from_millis(5);
        let samples = source.samples(
            D + Duration::from_millis(130),
            D + Duration::from_millis(330),
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            D + Duration::from_millis(130) + shift,
            RATE,
        ));

        let mut all_output = out_chunk_1;
        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 14));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        // should be aligned at 20-40ms range with test signal
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_shift_forward_5ms.wav",
        );

        // primed() produced 1024 frames (4 rubato runs of 256), so the drift is first seen when
        // producing frame 1024.
        let drift_start = 1024;
        let base_pts = D + Duration::from_millis(20);
        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..drift_start],
            source: &source.shifted(base_pts + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
        // Stretch ratio is `2 * MAX_STRETCH_RATIO * drift / STRETCH_THRESHOLD`, so the drift
        // decays with time constant `STRETCH_THRESHOLD / (2 * MAX_STRETCH_RATIO)` ≈ 488ms.
        assert_drift_converges(
            &all_output,
            &source,
            base_pts,
            drift_start,
            shift,
            STRETCH_THRESHOLD
                .to_duration_saturating()
                .div_f64(2.0 * MAX_STRETCH_RATIO),
            true,
        );
    }

    /// Write 200ms of continuous signal [130ms, 330ms)+D with PTS shifted
    /// backward by 5ms (input appears 5ms early). The overlap is below
    /// `SEAM_THRESHOLD`, so it is appended and the whole buffered input is placed 5ms
    /// earlier. The resampler should compress the input over several
    /// chunks to absorb the overlap.
    #[test]
    fn drift_shift_backward_5ms() {
        let (source, mut r, out_chunk_1) = primed();
        let shift = Duration::from_millis(5);
        let samples = source.samples(
            D + Duration::from_millis(130),
            D + Duration::from_millis(330),
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples),
            D + Duration::from_millis(130) - shift,
            RATE,
        ));

        let mut all_output = out_chunk_1;
        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 14));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_shift_backward_5ms.wav",
        );

        // primed() produced 1024 frames (4 rubato runs of 256), so the drift is first seen when
        // producing frame 1024.
        let drift_start = 1024;
        let base_pts = D + Duration::from_millis(20);
        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..drift_start],
            source: &source.shifted(base_pts + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
        // Squash ratio is `2 * MAX_STRETCH_RATIO * drift / SQUASH_THRESHOLD`, so the drift
        // decays with time constant `SQUASH_THRESHOLD / (2 * MAX_STRETCH_RATIO)` ≈ 6.1s.
        assert_drift_converges(
            &all_output,
            &source,
            base_pts,
            drift_start,
            shift,
            SQUASH_THRESHOLD
                .to_duration_saturating()
                .div_f64(2.0 * MAX_STRETCH_RATIO),
            false,
        );
    }

    /// Write 200ms of continuous signal [130ms, 330ms)+D, then call
    /// `get_samples` in 20ms chunks. Input is contiguous with primed()
    /// state — baseline for drift tests.
    #[test]
    fn drift_no_shift() {
        let (source, mut r, out_chunk_1) = primed();
        r.write_batch(source.batch(D + Duration::from_millis(130), Duration::from_millis(200)));

        let mut all_output = out_chunk_1;
        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 9));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &all_output], RATE, "running_drift_no_shift.wav");

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..],
            source: &source.shifted(D + Duration::from_millis(20) + SAMPLE48 * (start as u32 + 1)),
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
            D + Duration::from_millis(130),
            D + Duration::from_millis(230),
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples_1),
            D + Duration::from_millis(130) + shift,
            RATE,
        ));
        // Second 100ms batch: no offset (contiguous with first batch's real data)
        r.write_batch(source.batch(D + Duration::from_millis(230), Duration::from_millis(100)));

        let mut all_output = out_chunk_1;
        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 9));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_first_batch_offset_forward_5ms_second_no_offset.wav",
        );

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..],
            source: &source.shifted(D + Duration::from_millis(20) + SAMPLE48 * (start as u32 + 1)),
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
            D + Duration::from_millis(130),
            D + Duration::from_millis(230),
        );
        r.write_batch(InputAudioSamples::new(
            AudioSamples::Mono(samples_1),
            D + Duration::from_millis(130) - shift,
            RATE,
        ));
        // Second 100ms batch: no offset (contiguous with first batch's real data)
        r.write_batch(source.batch(D + Duration::from_millis(230), Duration::from_millis(100)));

        let mut all_output = out_chunk_1;
        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 9));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_drift_first_batch_offset_backward_5ms_second_no_offset.wav",
        );

        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..],
            source: &source.shifted(D + Duration::from_millis(20) + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
    }

    /// Write [180ms, 260ms)+D after primed (buffered end at 130ms). The
    /// 50ms gap is at least `SEAM_THRESHOLD`, so `write_batch` flushes the
    /// buffered input with 5ms of concealment and the gate places the new
    /// batch at its PTS once the flushed output is played. Output:
    /// - [20, 130)+D  = primed input
    /// - [130, 135)+D = concealment
    /// - [135, 180)+D = silence
    /// - [180, 240)+D = written batch, faded in
    ///
    /// While the flushed output still covers a request the resync is
    /// deferred.
    #[test]
    fn gap_flushes_and_restarts_at_batch_pts() {
        let (source, mut r, mut all_output) = primed();
        r.write_batch(source.batch(D + Duration::from_millis(180), Duration::from_millis(80)));
        assert!(r.needs_input_resync);

        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 1));
        assert!(
            r.needs_input_resync,
            "resync is deferred while output covers request"
        );
        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(60), 9));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_gap_flushes_and_restarts_at_batch_pts.wav",
        );

        let base_pts = D + Duration::from_millis(20);
        let start = FADE_IN + FIR_WINDOW;
        let input_end = 960 * 5 + 480;
        SignalAssertion {
            output: &all_output[start..(input_end - FIR_WINDOW)],
            source: &source.shifted(base_pts + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
        let batch_start = 960 * 8;
        SignalAssertion {
            output: &all_output[(input_end + 240 + FIR_WINDOW)..(batch_start - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        // Placement after a restart is exact to one frame. `SAMPLE48` is truncated to whole
        // nanoseconds, too imprecise to multiply by a frame index this large.
        let start = batch_start + FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..(960 * 11)],
            source: &source
                .shifted(base_pts + Duration::from_secs_f64((start + 1) as f64 / RATE as f64)),
        }
        .max_shift(SAMPLE48)
        .assert();
    }

    /// Write [130ms, 230ms)+D as 5 batches of 20ms continuous content,
    /// each with PTS 9ms after the previous batch's end. Each gap is below
    /// `SEAM_THRESHOLD`, so they are appended, but together they place the
    /// buffered input 45ms later — more than `STRETCH_THRESHOLD`. Drift
    /// control treats that as a discontinuity when producing frame 1024
    /// (41.33ms): it fades out the next 5ms of input and the gate restarts
    /// the rest 45ms later, faded in. Output:
    /// - [20, 41.33)+D    = source
    /// - [41.33, 46.67)+D = fade out, then the filter tail
    /// - [46.67, 91.33)+D = silence
    /// - [91.33, 240)+D   = source 45ms late, faded in
    #[test]
    fn accumulated_gaps_restart_after_gap() {
        let (source, mut r, mut all_output) = primed();
        for i in 0..5 {
            let content_start = D + Duration::from_millis(130) + Duration::from_millis(20) * i;
            let samples = source.samples(content_start, content_start + Duration::from_millis(20));
            r.write_batch(InputAudioSamples::new(
                AudioSamples::Mono(samples),
                content_start + Duration::from_millis(9) * (i + 1),
                RATE,
            ));
        }

        all_output.extend(read_chunks(&mut r, D + Duration::from_millis(40), 10));
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(
            &[&pad, &all_output],
            RATE,
            "running_accumulated_gaps_restart_after_gap.wav",
        );

        let base_pts = D + Duration::from_millis(20);
        let start = FADE_IN + FIR_WINDOW;
        let drift_start = 1024;
        SignalAssertion {
            output: &all_output[start..drift_start],
            source: &source.shifted(base_pts + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
        // Faded out input plus the filter tail (`output_delay`, 16 frames) flushed after it.
        let flush_end = drift_start + FADE_IN + 16;
        // The gate pads the rest of the input up to its PTS, 45ms (2160 frames) after the
        // content's original position.
        let restart = drift_start + FADE_IN + 2160;
        SignalAssertion {
            output: &all_output[(flush_end + FIR_WINDOW)..(restart - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        // Placement after a restart is exact to one frame. `SAMPLE48` is truncated to whole
        // nanoseconds, too imprecise to multiply by a frame index this large.
        let start = restart + FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..],
            source: &source.shifted(
                base_pts + Duration::from_secs_f64((start + 1) as f64 / RATE as f64)
                    - Duration::from_millis(45),
            ),
        }
        .max_shift(SAMPLE48 * 2)
        .assert();
    }

    /// Same setup as `primed()` but with PTS shifted up by 1000ms to leave
    /// room for backward drift. Writes [1010, 1130)+D, reads [1020, 1040)+D.
    /// Then writes 80 contiguous 20ms batches of audio from [1130, 2730)+D,
    /// each with PTS shifted backward by `(i+1) * 7.5ms`. Each batch is 20ms
    /// of audio but its PTS only advances by 12.5ms, so `input_buffer_end_pts`
    /// falls behind by 7.5ms per batch. Each overlap is below
    /// `SEAM_THRESHOLD`, so the batches are appended instead of starting a
    /// new timeline.
    ///
    /// After all writes the accumulated drift is 600ms. Read at
    /// [1040, 1060)+D → triggers DROP.
    #[test]
    fn drift_shift_backward_600ms() {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal_5s());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();
        r.write_batch(source.batch(D + Duration::from_millis(1010), Duration::from_millis(60)));
        r.write_batch(source.batch(D + Duration::from_millis(1070), Duration::from_millis(60)));
        let out_chunk_1 = mono(r.get_samples((
            D + Duration::from_millis(1020),
            D + Duration::from_millis(1040),
        )));

        // Each 20ms batch has PTS shifted backward by (i+1)*7.5ms — would
        // require squashing by 37.5% to handle without drops.
        for i in 0..80u64 {
            let content_start = D + Duration::from_millis(1130 + i * 20);
            let content_end = D + Duration::from_millis(1150 + i * 20);
            let samples = source.samples(content_start, content_end);
            r.write_batch(InputAudioSamples::new(
                AudioSamples::Mono(samples),
                content_start - Duration::from_micros((i + 1) * 7500),
                RATE,
            ));
        }
        // Each batch introduces 7.5ms of backward drift; after 80 batches the
        // total is 600ms — past SQUASH_THRESHOLD, triggering DROP.

        let chunk = mono(r.get_samples((
            D + Duration::from_millis(1040),
            D + Duration::from_millis(1060),
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

        // The first 64 samples of the second request are leftover from the
        // first request's output_buffer, they play before the drop.
        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &all_output[start..(960 + 64 - FIR_WINDOW)],
            source: &source
                .shifted(D + Duration::from_millis(1020) + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();

        // After DROP, content is shifted by ~600ms relative to the normal
        // timeline. `splice::drop_frames` moves the cut by up to
        // `SEARCH_DURATION` (5ms) to where the waveform matches best and
        // crossfades over the first `CROSSFADE_DURATION` (5ms) after it.
        let after_crossfade = 64 + FADE_IN + FIR_WINDOW;
        let expected = source.shifted(
            D + Duration::from_millis(1040)
                + SAMPLE48 * (after_crossfade as u32 + 1)
                + Duration::from_millis(600),
        );
        let (cut_error, rms) = measure_offset(
            &chunk[after_crossfade..],
            &expected,
            Duration::from_millis(5),
        );
        assert!(
            rms < 0.01 && cut_error.abs() <= 0.005,
            "dropped {:.3}ms instead of 600ms ± 5ms (rms {rms:.4})",
            600.0 + cut_error * 1e3
        );
    }
}

/// `get_samples` calls after the input ran out. The common setup writes
/// [10, 70)+D and reads [20, 80)+D in three 20ms requests. The gate needs
/// input far enough past the request end, so the input can only run out
/// in the third request, where it is played to its end and flushed with
/// 5ms of concealment, after which:
/// - the input buffer is empty and the resampler is resyncing,
/// - `output_buffer` is empty, the concealment ended at 75ms.
///
/// Each test then writes new input and reads from (80, 100)+D on. The
/// concatenated output of all `get_samples` calls is dumped to a WAV file
/// for inspection.
mod drained {
    use super::*;

    /// Common init: [10, 70)+D input, then reads of [20, 80)+D.
    fn primed() -> (SignalSource, InputResampler, Vec<f64>) {
        try_init_logger();
        let source = SignalSource::new(RATE, test_signal());
        let mut r = InputResampler::new(RATE, RATE, AudioChannels::Mono).unwrap();
        r.write_batch(source.batch(D + Duration::from_millis(10), Duration::from_millis(30)));
        r.write_batch(source.batch(D + Duration::from_millis(40), Duration::from_millis(30)));
        let prev = read_chunks(&mut r, D + Duration::from_millis(20), 3);
        (source, r, prev)
    }

    /// Assert primed() output: the input [20, 70)+D, 5ms of concealment
    /// and silence until 80ms.
    #[test]
    fn primed_output() {
        let (source, r, all_output) = primed();

        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &all_output], RATE, "drained_primed_output.wav");

        let start = FADE_IN + FIR_WINDOW;
        let input_end = 960 * 2 + 480;
        SignalAssertion {
            output: &all_output[start..(input_end - FIR_WINDOW)],
            source: &source.shifted(D + Duration::from_millis(20) + SAMPLE48 * (start as u32 + 1)),
        }
        .assert();
        SignalAssertion {
            output: &all_output[(input_end + 240 + FIR_WINDOW)..],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        assert!(r.needs_input_resync);
        assert_eq!(r.resampler_input_buffer.frames(), 0);
        assert_eq!(r.output_buffer.frames(), 0);
    }

    /// Read [80, 100)+D on drained state — no new input written. Output
    /// is silence.
    #[test]
    fn no_new_input() {
        let (_source, mut r, _prev) = primed();

        let chunk = read_chunks(&mut r, D + Duration::from_millis(80), 1);

        SignalAssertion {
            output: &chunk,
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
    }

    /// Write [70, 150)+D after drained state, contiguous with the flushed
    /// input, then read [80, 120)+D. The part of the new input before 80ms
    /// is already in the past: the gate drains it and fades in the rest at
    /// 80ms.
    #[test]
    fn input_continues() {
        let (source, mut r, mut all_output) = primed();

        r.write_batch(source.batch(D + Duration::from_millis(70), Duration::from_millis(80)));

        let chunks = read_chunks(&mut r, D + Duration::from_millis(80), 2);
        all_output.extend_from_slice(&chunks);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &all_output], RATE, "drained_input_continues.wav");

        // Placement after a restart is exact to one frame.
        let start = FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &chunks[start..],
            source: &source.shifted(D + Duration::from_millis(80) + SAMPLE48 * (start as u32 + 1)),
        }
        .max_shift(SAMPLE48)
        .assert();
    }

    /// Write [90, 170)+D after drained state, then read [80, 120)+D. The
    /// gate pads 10ms of silence and fades in the input at 90ms.
    #[test]
    fn input_after_gap() {
        let (source, mut r, mut all_output) = primed();

        r.write_batch(source.batch(D + Duration::from_millis(90), Duration::from_millis(80)));

        let chunks = read_chunks(&mut r, D + Duration::from_millis(80), 2);
        all_output.extend_from_slice(&chunks);
        let pad = silence_samples(RATE, Duration::from_millis(20));
        dump_wav(&[&pad, &all_output], RATE, "drained_input_after_gap.wav");

        SignalAssertion {
            output: &chunks[..(480 - FIR_WINDOW)],
            source: &SignalSource::new(RATE, silence()),
        }
        .assert();
        // Placement after a restart is exact to one frame.
        let start = 480 + FADE_IN + FIR_WINDOW;
        SignalAssertion {
            output: &chunks[start..],
            source: &source.shifted(D + Duration::from_millis(80) + SAMPLE48 * (start as u32 + 1)),
        }
        .max_shift(SAMPLE48)
        .assert();
    }
}
