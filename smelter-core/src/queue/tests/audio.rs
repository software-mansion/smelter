//! Audio variants of the scenarios from [`super::video`]. The audio queue
//! differs from video in a few important ways:
//!
//! - Output is a 20ms chunk with a PTS range, produced at real-time pace (a
//!   chunk is never produced before its start PTS). Input batches are not cut
//!   to the chunk grid: a chunk carries every batch (as sent, PTS shifted by
//!   the offset) with start PTS lower than the chunk end + 80ms
//!   (`MIXER_STRETCH_BUFFER`), so batches are delivered up to ~100ms ahead of
//!   their PTS and everything buffered around the offset point is delivered at
//!   once. The audio mixer is responsible for placing them on the timeline.
//! - A required input reports ready only when it is buffered up to the chunk
//!   end + 80ms, so it stalls the queue until ~100ms of input is buffered
//!   ahead of the chunk being produced.
//! - After the queue starts nothing is dropped or repeated: every batch is
//!   delivered exactly once even if it is late (video drops late frames and
//!   repeats the last frame on optional inputs). Before the start only the
//!   `None` offset drops old batches.
//! - A chunk always contains an entry for the input (with an empty batch list
//!   when the input delivered nothing) and `required` mirrors the input's
//!   required flag, while an empty video batch has no entry and
//!   `required: false`.
//!
//! All batches sent by tests are 20ms long ([`BATCH_DURATION`]), so input
//! batches line up 1:1 with output chunks and expectations stay readable.

use std::{thread::sleep, time::Duration};

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{
    AudioBatch, BATCH_DURATION, InputSamples, OFFSET, TestInput, TestQueue, TestQueueOptions,
    assert_audio_batch_eq, ms, samples,
};

mod required_input {
    use super::*;

    /// A chunk with sample batches from the required "input_1". PTS ranges are
    /// relative to the queue start. An empty `batches` list means the input
    /// delivered nothing for this chunk (the entry is still present and the
    /// chunk is still required, unlike an empty video batch).
    fn chunk(start_pts: Duration, batches: Vec<(Duration, Duration)>) -> AudioBatch {
        AudioBatch {
            start_pts,
            end_pts: start_pts + BATCH_DURATION,
            required: true,
            samples: samples([("input_1", InputSamples::Batches(batches))]),
        }
    }

    /// Create a queue with a single required audio-only input ("input_1").
    /// The queue is not started yet, so samples can be sent before start.
    fn create_queue_with_audio_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let queue = TestQueue::new(TestQueueOptions::default());
        let input = queue.add_input(
            "input_1",
            QueueInputOptions {
                required: true,
                ..Default::default()
            },
            QueueTrackOptions {
                video: false,
                audio: true,
                offset,
            },
        );
        (queue, input)
    }

    /// Like [`create_queue_with_audio_input`], but desync the clocks and start
    /// the queue.
    fn start_queue_with_audio_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let (mut queue, input) = create_queue_with_audio_input(offset);

        // desync regular clock from queue clock
        sleep(OFFSET);

        queue.start();
        (queue, input)
    }

    #[test]
    fn offset_from_start_delivered_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);
        input.send_samples(ms(100), BATCH_DURATION);

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // [40, 60) is the first chunk that is not entirely before the offset
        // point, so it pops everything within the 80ms stretch window (input
        // PTS below 80ms, queue PTS below 140ms) at once
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // steady state: one batch per chunk, delivered 80ms ahead of its PTS
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(140), ms(160))]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![(ms(160), ms(180))]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // out of data: the required input stalls the queue ([100, 120) needs
        // the input buffered up to 140ms)
        sleep(ms(20));
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit before

        // empty chunks are produced while the input has no data; [40, 60) is
        // not one of them because its end PTS already reaches the offset point
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        // [60, 80) needs the input buffered up to 100ms
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(140), ms(160))]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(200));
        // empty chunks before the offset point, then the queue stalls waiting
        // for the required input
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);

        // the queue catches up as far as the buffered data allows; no batch is
        // skipped, they are all delivered even though their PTS is in the past
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(140), ms(160))]),
            Duration::ZERO,
        );
        // [80, 100) needs the input buffered up to 120ms
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late_first_packet_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // the first batch switches the input from "ready until the offset
        // point" to coverage-based readiness
        input.send_samples(ms(0), BATCH_DURATION);

        // even the empty pre-offset chunks are withheld now: [20, 40) waits
        // until the input is buffered up to 80ms (in
        // `offset_from_start_delivered_late` the 20ms chunk is produced)
        sleep(ms(200));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        // [60, 80) needs the input buffered up to 100ms
        assert!(queue.next_audio_batch().is_none());
    }

    //
    // Pts offsets are relative to the sync point (queue creation): `Pts(OFFSET + d)`
    // places the track zero ~d after start, `Pts(OFFSET - d)` ~d before start.
    //
    // Unlike `FromStart`, a `Pts` track has its offset resolved from the very
    // beginning: with no data the queue stalls from the first chunk instead of
    // producing empty chunks, and chunks before the offset point are not
    // filtered, so the first chunk carries everything within the stretch
    // window.
    //
    // The video `first_packet_early` variants (the first packet initializes
    // the input early, the rest arrives too late) are not repeated for `Pts`:
    // the offset is fixed at construction, so there is nothing for the first
    // packet to initialize, and a single batch never satisfies the stretch
    // buffer readiness — the output is identical to `delivered_late`. See the
    // `FromStart` and `None` variants where the first packet does change the
    // behavior.
    //

    #[test]
    fn offset_pts_after_start_delivered_early() {
        let (queue, input) =
            start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // [40, 60) needs the input buffered a bit past 80ms (the queue clock is
        // desynced from OFFSET by a small positive epsilon)
        sleep(ms(20));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(140), ms(160))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time() {
        let (queue, input) =
            start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        // a required `Pts` input stalls the queue from the first chunk until
        // data arrives (no empty chunks, unlike `FromStart` or `None`)
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            ms(2),
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(140), ms(160))]),
            ms(2),
        );
        // [60, 80) needs the input buffered past 100ms
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_late() {
        let (queue, input) =
            start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(200));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            ms(2),
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        // [40, 60) needs the input buffered past 80ms
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(140), ms(160))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    //
    // The video aligned/not-aligned before-start variants are not repeated:
    // audio doesn't pick one batch per chunk the way video picks frames, every
    // batch is forwarded, so alignment only shifts the reported ranges.
    //

    #[test]
    fn offset_pts_before_start_delivered_early() {
        // track zero is ~40ms before the queue start: batches with PTS below
        // ~40ms are already in the past at start, but unlike video they are
        // not dropped, they are delivered with the first chunk (their queue
        // PTS saturates to 0 in the summary)
        let (mut queue, input) =
            create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        // ~120ms is the most that can be buffered before start without
        // blocking (100ms buffer + one batch in the channel)
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);
        input.send_samples(ms(100), BATCH_DURATION);

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        // [0, 20) needs the input buffered up to ~140ms (chunk end + stretch
        // buffer, shifted by the -40ms offset), so 120ms is not enough
        sleep(ms(1));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(120), BATCH_DURATION);
        input.send_samples(ms(140), BATCH_DURATION);

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(0), ms(0)),
                    (ms(0), ms(0)),
                    (ms(0), ms(20)),
                    (ms(20), ms(40)),
                    (ms(40), ms(60)),
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_late() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        sleep(ms(50) + OFFSET);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);
        input.send_samples(ms(100), BATCH_DURATION);

        queue.start();

        // [0, 20) needs the input buffered up to ~150ms (the queue started
        // 50ms after the track zero)
        sleep(ms(1));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(120), BATCH_DURATION);
        input.send_samples(ms(140), BATCH_DURATION);

        // everything is delivered with the first chunk, batches from before
        // the queue start saturate to 0 in the summary
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(0), ms(0)),
                    (ms(0), ms(0)),
                    (ms(0), ms(10)),
                    (ms(10), ms(30)),
                    (ms(30), ms(50)),
                    (ms(50), ms(70)),
                    (ms(70), ms(90)),
                    (ms(90), ms(110)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    //
    // None offset: the offset is not known upfront, it locks to the start PTS
    // of the chunk that first observes a batch. Until then a required input
    // with a `None` offset reports ready (producing empty chunks) instead of
    // stalling like a `Pts` offset. Once locked, the offset is on the chunk
    // grid, so expectations are exact.
    //

    #[test]
    fn offset_none_after_start_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        // input reports ready with no data: empty chunks until a batch arrives
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // the offset locks to the chunk that first observed a batch ([60, 80)),
        // but the chunk still needs the input buffered up to 100ms
        sleep(ms(4));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                    (ms(140), ms(160)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_on_time_first_non_zero() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(5), BATCH_DURATION);
        input.send_samples(ms(25), BATCH_DURATION);
        input.send_samples(ms(45), BATCH_DURATION);
        input.send_samples(ms(65), BATCH_DURATION);

        sleep(ms(4));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(85), BATCH_DURATION);
        sleep(ms(1));
        // none offset maps input 0ms to the chunk start (queue 60ms), so the
        // first batch (input PTS 5ms) starts at queue 65ms
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(65), ms(85)),
                    (ms(85), ms(105)),
                    (ms(105), ms(125)),
                    (ms(125), ms(145)),
                    (ms(145), ms(165)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_on_time_with_gap() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // hole between input 20ms and 40ms
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);

        // readiness only checks the newest buffered PTS, so the hole doesn't
        // stall the queue and the gap is preserved in the delivered ranges
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(80)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                    (ms(140), ms(160)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // [80, 100) needs the input buffered up to 120ms
        sleep(ms(20));
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_late_first_packet_early() {
        // without an early packet a `None` offset stream can't be "late": the
        // offset locks wherever data first shows up. The early batch anchors
        // the offset, so the batches that arrive 200ms later are late relative
        // to that lock.
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(30));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // locks the offset to the next unproduced chunk: input 0ms ↔ queue 40ms
        input.send_samples(ms(0), BATCH_DURATION);

        // required input with only 20ms of coverage: the queue stalls at
        // [40, 60) instead of producing empty chunks
        sleep(ms(200));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);

        // everything is delivered with the mapping anchored by the early
        // batch, even though those PTS are long in the past
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(40), ms(60)),
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        // [60, 80) needs the input buffered up to 120ms
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_before_start() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // batches arrive 30ms before the queue starts; the offset locks at the
        // pre-start cleanup tick (~send time), and pre-start cleanup drops
        // batches whose start PTS is already in the past (0ms and 20ms here)
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        sleep(ms(30));
        queue.start();

        // [0, 20) needs the input buffered up to ~130ms
        sleep(ms(1));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        input.send_samples(ms(100), BATCH_DURATION);
        input.send_samples(ms(120), BATCH_DURATION);

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(10), ms(30)),
                    (ms(30), ms(50)),
                    (ms(50), ms(70)),
                    (ms(70), ms(90)),
                    (ms(90), ms(110)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }
}

mod optional_input {
    use super::*;

    /// A chunk with sample batches from the optional "input_1". PTS ranges are
    /// relative to the queue start.
    fn chunk(start_pts: Duration, batches: Vec<(Duration, Duration)>) -> AudioBatch {
        AudioBatch {
            start_pts,
            end_pts: start_pts + BATCH_DURATION,
            required: false,
            samples: samples([("input_1", InputSamples::Batches(batches))]),
        }
    }

    /// Create a queue with a single optional audio-only input ("input_1").
    /// The queue is not started yet, so samples can be sent before start.
    fn create_queue_with_audio_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let queue = TestQueue::new(TestQueueOptions::default());
        let input = queue.add_input(
            "input_1",
            QueueInputOptions {
                required: false,
                ..Default::default()
            },
            QueueTrackOptions {
                video: false,
                audio: true,
                offset,
            },
        );
        (queue, input)
    }

    /// Like [`create_queue_with_audio_input`], but desync the clocks and start
    /// the queue.
    fn start_queue_with_audio_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let (mut queue, input) = create_queue_with_audio_input(offset);

        // desync regular clock from queue clock
        sleep(OFFSET);

        queue.start();
        (queue, input)
    }

    #[test]
    fn offset_from_start_delivered_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);
        input.send_samples(ms(100), BATCH_DURATION);

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(140), ms(160))]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![(ms(160), ms(180))]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // out of data: empty chunks keep flowing (video would repeat the last
        // frame, audio never repeats a batch)

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(120), vec![]),
            Duration::ZERO,
        );
    }

    #[test]
    fn offset_from_start_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit before

        // an optional input never stalls the queue, so [40, 60) is produced
        // (empty) even though a required input would hold it back
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // no coverage requirement for optional inputs: everything within the
        // stretch window is delivered as soon as [60, 80) is due
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(140), ms(160))]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(98));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // late batches are delivered anyway (video would skip late frames)
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(100),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(120), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    //
    // Pts offsets are relative to the sync point (queue creation): `Pts(OFFSET + d)`
    // places the track zero ~d after start, `Pts(OFFSET - d)` ~d before start.
    //
    // The video `first_packet_early` variants are not repeated: an optional
    // input pops whatever arrived once a chunk is due, so an early packet is
    // just a smaller `delivered_early` scenario.
    //

    #[test]
    fn offset_pts_after_start_delivered_early() {
        // batches are sent before the queue starts: an optional input pops
        // whatever arrived when a chunk becomes due, so sending right after
        // start would race the first chunk
        let (mut queue, input) =
            create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(140), ms(160))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time() {
        let (queue, input) =
            start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        // empty chunks flow because the optional input never stalls the queue
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_late() {
        let (queue, input) =
            start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(78));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // batches for queue PTS 60..140ms arrive after those chunks were
        // already produced; they are still delivered with the next chunk
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(80),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_early() {
        // track zero is ~40ms before the queue start: batches with PTS below
        // ~40ms are already in the past at start; they are not dropped (video
        // drops them), their queue PTS saturates to 0 in the summary
        let (mut queue, input) =
            create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        // no stall for an optional input, [0, 20) delivers everything
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(0), ms(0)),
                    (ms(0), ms(0)),
                    (ms(0), ms(20)),
                    (ms(20), ms(40)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_late() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        sleep(ms(50) + OFFSET);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        queue.start();

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(0), ms(0)),
                    (ms(0), ms(0)),
                    (ms(0), ms(10)),
                    (ms(10), ms(30)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    //
    // None offset: the offset is not known upfront, it locks to the start PTS
    // of the chunk that first observes a batch.
    //

    #[test]
    fn offset_none_after_start_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // the offset locks to the chunk that first observed a batch ([60, 80));
        // unlike a required input there is no stall waiting for more data
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(140), ms(160))]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_on_time_first_non_zero() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(5), BATCH_DURATION);
        input.send_samples(ms(25), BATCH_DURATION);
        input.send_samples(ms(45), BATCH_DURATION);
        input.send_samples(ms(65), BATCH_DURATION);

        // none offset maps input 0ms to the chunk start (queue 60ms), so the
        // first batch (input PTS 5ms) starts at queue 65ms
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(65), ms(85)),
                    (ms(85), ms(105)),
                    (ms(105), ms(125)),
                    (ms(125), ms(145)),
                ],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_late_first_packet_early() {
        // the early batch anchors the offset (input 0ms ↔ queue 40ms); the
        // rest of the stream arrives too late but is still delivered with that
        // mapping
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(30));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![]),
            Duration::ZERO,
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        // locks the offset to the next unproduced chunk: input 0ms ↔ queue 40ms
        input.send_samples(ms(0), BATCH_DURATION);

        // an optional input doesn't wait for coverage, the early batch is
        // delivered on time
        sleep(ms(12));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(40), ms(60))]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // batches for queue PTS 60..120ms are delivered with the [100, 120)
        // chunk, they keep the mapping anchored by the early batch
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(100),
                vec![(ms(60), ms(80)), (ms(80), ms(100)), (ms(100), ms(120))],
            ),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_before_start() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // batches arrive 30ms before the queue starts; the offset locks at the
        // pre-start cleanup tick (~send time), and pre-start cleanup drops
        // batches whose start PTS is already in the past (0ms and 20ms here)
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        sleep(ms(30));
        queue.start();

        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(0), vec![(ms(10), ms(30)), (ms(30), ms(50))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![]),
            Duration::ZERO,
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(50), ms(70))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }
}
