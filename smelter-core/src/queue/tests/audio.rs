//! Audio variants of the scenarios from [`super::video`]. The audio queue
//! differs from video in a few important ways:
//!
//! - Output is a 20ms chunk with a PTS range, produced at real-time pace (a
//!   chunk is never produced before its start PTS). Input batches are not cut
//!   to the chunk grid: a chunk carries every batch (as sent, PTS shifted by
//!   the offset) with start PTS lower than the chunk end + 80ms
//!   (`MIXER_STRETCH_BUFFER`), so batches are delivered up to ~100ms ahead of
//!   their PTS. The audio mixer is responsible for placing them on the
//!   timeline.
//! - A required input reports ready only when it is buffered up to the chunk
//!   end + 80ms, so it stalls the queue until ~100ms of input is buffered
//!   ahead of the chunk being produced. Since a chunk drains the buffer up to
//!   the same point, the input becomes not-ready again after every chunk (even
//!   a chunk with nothing left to deliver) until newer data arrives.
//! - After the queue starts nothing is dropped or repeated: every batch is
//!   delivered exactly once even if it is late (video drops late frames and
//!   repeats the last frame on optional inputs). Before the start the cleanup
//!   tick drops batches whose start PTS is already in the past for `Pts` and
//!   `None` offsets (a `FromStart` offset is not resolvable before start, so
//!   nothing is dropped).
//! - A chunk always contains an entry for the input (with an empty batch list
//!   when the input delivered nothing) and `required` mirrors the input's
//!   required flag; video batches behave the same way.
//!
//! Input batches sent by tests are 15ms long ([`INPUT_BATCH_DURATION`]) while
//! output chunks are 20ms, the same cadence as the video tests (15ms input
//! frames, 20ms output batches), so input batches don't line up with the
//! chunk grid.

use std::{thread::sleep, time::Duration};

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{
    AudioBatch, BATCH_DURATION, INPUT_BATCH_DURATION, InputSamples, OFFSET, TestInput, TestQueue,
    TestQueueOptions, assert_audio_batch_eq, assert_audio_batch_eq_with_tolerance,
    assert_empty_audio_batch, ms, samples,
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
            samples: samples([("input_1", InputSamples::batches(batches))]),
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

    /// The offset is larger than the stretch window, so the batches sent right
    /// after start are withheld until the first chunk that reaches the offset
    /// point.
    #[test]
    fn offset_from_start_delivered_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(100)));

        // the whole stream is sent right after start (queue PTS 100-220ms)
        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        input.send_samples(ms(105), INPUT_BATCH_DURATION);

        // chunks that end before the offset point deliver nothing even though
        // everything is already buffered
        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(60), true);
        assert!(queue.next_audio_batch().is_none());

        // [80, 100) is the first chunk that reaches the offset point: it pops
        // everything within the 80ms stretch window (input PTS below 80ms)
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(80), // output batch 80ms-100ms
                vec![
                    (ms(100), ms(115)),
                    (ms(115), ms(130)),
                    (ms(130), ms(145)),
                    (ms(145), ms(160)),
                    (ms(160), ms(175)),
                    (ms(175), ms(190)), // audio is send 80ms ahead of time so this is the last
                                        // batch in range
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        // one batch per chunk, delivered ~100ms ahead of its PTS
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(190), ms(205))]),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(120), vec![(ms(205), ms(220))]),
        );
        assert!(queue.next_audio_batch().is_none());

        // out of data: the required input stalls the queue ([140, 160) needs
        // the input buffered up to input PTS 140ms, only 120ms was sent)
        sleep(ms(20));
        assert!(queue.next_audio_batch().is_none());
    }

    /// Sent early, but within the stretch window, so nothing is withheld:
    /// verifies that [0, 20) and [20, 40) stay empty with data already
    /// buffered.
    #[test]
    fn offset_from_start_delivered_slightly_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        input.send_samples(ms(105), INPUT_BATCH_DURATION);

        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
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
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        // steady state: one batch per chunk, delivered ~100ms ahead of its PTS
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(150), ms(165))]),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![(ms(165), ms(180))]),
        );
        assert!(queue.next_audio_batch().is_none());

        // out of data: the required input stalls the queue ([100, 120) needs
        // the input buffered up to 140ms, only 120ms was sent)
        sleep(ms(20));
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit before

        // empty chunks are produced while the input has no data; [40, 60) is
        // not one of them because its end PTS already reaches the offset point
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
        );
        // [60, 80) needs the input buffered up to 100ms, only 90ms was sent
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(150), ms(165))]),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(200));
        // empty chunks before the offset point, then the queue stalls waiting
        // for the required input
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);

        // the queue catches up as far as the buffered data allows; no batch is
        // skipped, they are all delivered even though their PTS is in the past
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
        );
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(150), ms(165))]),
        );
        // [80, 100) needs the input buffered up to 120ms, only 105ms was sent
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late_first_packet_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert!(queue.next_audio_batch().is_none());

        // the first batch switches the input from "ready until the offset
        // point" to coverage-based readiness
        input.send_samples(ms(0), INPUT_BATCH_DURATION);

        // even the empty pre-offset chunks are withheld now: [20, 40) waits
        // until the input is buffered up to 60ms (in
        // `offset_from_start_delivered_late` the 20ms chunk is produced)
        sleep(ms(200));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
        );
        // [60, 80) needs the input buffered up to 100ms, only 90ms was sent
        assert!(queue.next_audio_batch().is_none());
    }

    //
    // Pts offsets are relative to the sync point (queue creation): `Pts(OFFSET + d)`
    // places the track zero ~d after start, `Pts(OFFSET - d)` ~d before start.
    //
    // Unlike `FromStart`, a `Pts` track has its offset resolved from the very
    // beginning: with no data the queue stalls from the first chunk instead of
    // producing empty chunks, and chunks before the offset point are not
    // filtered: every chunk delivers the batches within its own stretch
    // window (queue PTS below chunk end + 80ms), so the head of the stream is
    // metered out from the first chunk instead of being withheld until the
    // offset point.
    //
    // The video `first_packet_early` variants (the first packet initializes
    // the input early, the rest arrives too late) are not repeated for `Pts`:
    // the offset is fixed at construction, so there is nothing for the first
    // packet to initialize, and a single batch doesn't satisfy the first
    // chunk's coverage requirement for these offsets — the output is
    // identical to `delivered_late`. See the `FromStart` and `None` variants
    // where the first packet does change the behavior.
    //

    /// The offset point is more than the stretch window ahead of the queue
    /// start. Unlike `FromStart`, chunks before the offset point still
    /// deliver: each chunk pops the batches within its own stretch window
    /// (queue PTS below chunk end + 80ms), so the head of the stream is
    /// metered out a batch or two per chunk instead of being withheld until
    /// the offset point. Only [0, 20) is empty: its window ends before the
    /// track zero.
    #[test]
    fn offset_pts_after_start_delivered_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(103)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);

        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(20),
                vec![(ms(103), ms(103 + 15)), (ms(103 + 15), ms(103 + 30))],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(103 + 30), ms(103 + 45))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(103 + 45), ms(103 + 60))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        // the 20ms chunk grid overtakes the 15ms batch grid: two batches fall
        // within the [80, 100) window
        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(80),
                vec![(ms(103 + 60), ms(103 + 75)), (ms(103 + 75), ms(103 + 90))],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(103 + 90), ms(103 + 105))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        // out of data: [120, 140) needs the input buffered up to ~117ms, only
        // 105ms was sent
        sleep(ms(20));
        assert!(queue.next_audio_batch().is_none());
    }

    /// Sent early, but the offset is within the stretch window: the first
    /// chunk delivers the whole head of the stream (queue PTS below 100ms),
    /// later chunks meter out the rest.
    #[test]
    fn offset_pts_after_start_delivered_slightly_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(63)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);

        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(63), ms(63 + 15)),
                    (ms(63 + 15), ms(63 + 30)),
                    (ms(63 + 30), ms(63 + 45)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![(ms(63 + 45), ms(63 + 60))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![(ms(63 + 60), ms(63 + 75)), (ms(63 + 75), ms(63 + 90))],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(63 + 90), ms(63 + 105))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(63)));

        sleep(ms(58));
        // a required `Pts` input stalls the queue from the first chunk until
        // data arrives (no empty chunks, unlike `FromStart` or `None`)
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // the queue catches up chunk by chunk, each delivering its own
        // stretch window
        sleep(ms(4));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(63), ms(63 + 15)),
                    (ms(63 + 15), ms(63 + 30)),
                    (ms(63 + 30), ms(63 + 45)),
                ],
            ),
            ms(2),
        );
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![(ms(63 + 45), ms(63 + 60))]),
            ms(2),
        );
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![(ms(63 + 60), ms(63 + 75)), (ms(63 + 75), ms(63 + 90))],
            ),
            ms(2),
        );
        // [60, 80) needs the input buffered up to ~97ms, only 90ms was sent
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(63 + 90), ms(63 + 105))]),
            ms(2),
        );
        // [80, 100) stalls again: it needs the input buffered up to ~117ms
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(63)));

        sleep(ms(200));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // the queue catches up as far as the buffered data allows, each chunk
        // delivering its own stretch window
        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(63), ms(63 + 15)),
                    (ms(63 + 15), ms(63 + 30)),
                    (ms(63 + 30), ms(63 + 45)),
                ],
            ),
            ms(2),
        );
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![(ms(63 + 45), ms(63 + 60))]),
            ms(2),
        );
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![(ms(63 + 60), ms(63 + 75)), (ms(63 + 75), ms(63 + 90))],
            ),
            ms(2),
        );
        // [60, 80) needs the input buffered up to ~97ms, only 90ms was sent
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(63 + 90), ms(63 + 105))]),
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
        // track zero is ~40ms before the queue start: like for video, batches
        // that are already in the past get dropped by the pre-start cleanup
        // (input PTS 0-30ms here, the cleanup compares batch start PTS)
        let (mut queue, input) =
            create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        input.send_samples(ms(105), INPUT_BATCH_DURATION);
        input.send_samples(ms(120), INPUT_BATCH_DURATION);
        input.send_samples(ms(135), INPUT_BATCH_DURATION);

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        // batches 0-30ms were dropped before start, the rest is delivered
        // with the first chunk ([0, 20) pops everything below queue PTS
        // 100ms, i.e. input PTS below ~140ms)
        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(45 - 40), ms(60 - 40)),
                    (ms(60 - 40), ms(75 - 40)),
                    (ms(75 - 40), ms(90 - 40)),
                    (ms(90 - 40), ms(105 - 40)),
                    (ms(105 - 40), ms(120 - 40)),
                    (ms(120 - 40), ms(135 - 40)),
                    (ms(135 - 40), ms(150 - 40)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_late() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        sleep(ms(53) + OFFSET);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        input.send_samples(ms(105), INPUT_BATCH_DURATION);
        input.send_samples(ms(120), INPUT_BATCH_DURATION);
        input.send_samples(ms(135), INPUT_BATCH_DURATION);
        input.send_samples(ms(150), INPUT_BATCH_DURATION);

        // sends are asynchronous: give the relay and the pre-start cleanup a
        // moment to process the batches before the queue starts (the queue
        // ends up starting ~55ms after the track zero)
        sleep(ms(2));
        queue.start();

        // batches 0-45ms (before the effective start point) were dropped by
        // the pre-start cleanup, the rest is delivered with the first chunk
        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(60 - 55), ms(75 - 55)),
                    (ms(75 - 55), ms(90 - 55)),
                    (ms(90 - 55), ms(105 - 55)),
                    (ms(105 - 55), ms(120 - 55)),
                    (ms(120 - 55), ms(135 - 55)),
                    (ms(135 - 55), ms(150 - 55)),
                    (ms(150 - 55), ms(165 - 55)),
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
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // the offset locks to the chunk that first observed a batch ([60, 80)),
        // but the chunk still needs the input buffered up to 100ms
        sleep(ms(4));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                    (ms(150), ms(165)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_on_time_first_non_zero() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(5), INPUT_BATCH_DURATION);
        input.send_samples(ms(20), INPUT_BATCH_DURATION);
        input.send_samples(ms(35), INPUT_BATCH_DURATION);
        input.send_samples(ms(50), INPUT_BATCH_DURATION);
        input.send_samples(ms(65), INPUT_BATCH_DURATION);
        input.send_samples(ms(80), INPUT_BATCH_DURATION);

        sleep(ms(4));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(95), INPUT_BATCH_DURATION);
        sleep(ms(1));
        // none offset maps input 0ms to the chunk start (queue 60ms), so the
        // first batch (input PTS 5ms) starts at queue 65ms
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(65), ms(80)),
                    (ms(80), ms(95)),
                    (ms(95), ms(110)),
                    (ms(110), ms(125)),
                    (ms(125), ms(140)),
                    (ms(140), ms(155)),
                    (ms(155), ms(170)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_on_time_with_gap() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(1));
        // hole between input 15ms and 30ms
        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);

        // readiness only checks the newest buffered PTS, so the hole doesn't
        // stall the queue and the gap is preserved in the delivered ranges
        sleep(ms(2));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(75)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                    (ms(150), ms(165)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        // [80, 100) needs the input buffered up to 120ms, only 105ms was sent
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
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert!(queue.next_audio_batch().is_none());

        // locks the offset to the next unproduced chunk: input 0ms ↔ queue 40ms
        input.send_samples(ms(0), INPUT_BATCH_DURATION);

        // required input with only 15ms of coverage: the queue stalls at
        // [40, 60) instead of producing empty chunks
        sleep(ms(200));
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);

        // everything is delivered with the mapping anchored by the early
        // batch, even though those PTS are long in the past
        sleep(ms(1));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(40), ms(55)),
                    (ms(55), ms(70)),
                    (ms(70), ms(85)),
                    (ms(85), ms(100)),
                    (ms(100), ms(115)),
                    (ms(115), ms(130)),
                    (ms(130), ms(145)),
                ],
            ),
        );
        // [60, 80) needs the input buffered up to 120ms, only 105ms was sent
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_before_start() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // batches arrive 25ms before the queue starts; the offset locks at the
        // pre-start cleanup tick (~send time), and pre-start cleanup drops
        // batches whose start PTS is already in the past (0ms and 15ms here)
        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        input.send_samples(ms(105), INPUT_BATCH_DURATION);
        input.send_samples(ms(120), INPUT_BATCH_DURATION);
        input.send_samples(ms(135), INPUT_BATCH_DURATION);

        sleep(ms(25));
        queue.start();

        // [0, 20) pops everything below queue PTS 100ms (input PTS below
        // ~125ms)
        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(30 - 25), ms(45 - 25)),
                    (ms(45 - 25), ms(60 - 25)),
                    (ms(60 - 25), ms(75 - 25)),
                    (ms(75 - 25), ms(90 - 25)),
                    (ms(90 - 25), ms(105 - 25)),
                    (ms(105 - 25), ms(120 - 25)),
                    (ms(120 - 25), ms(135 - 25)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    /// EOS is delivered on the chunk that pops the final batches, together
    /// with those batches.
    #[test]
    fn offset_from_start_eos_with_last_batches() {
        let (queue, mut input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(100)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.end_audio();

        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(60), true);
        assert!(queue.next_audio_batch().is_none());

        // [80, 100) is the first chunk that reaches the offset point; it pops
        // both batches, draining the stream: EOS on the same chunk
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &AudioBatch {
                start_pts: ms(80),
                end_pts: ms(100),
                required: true,
                samples: samples([(
                    "input_1",
                    InputSamples::batches_eos(vec![(ms(100), ms(115)), (ms(115), ms(130))]),
                )]),
            },
        );
        assert!(queue.next_audio_batch().is_none());

        // after EOS the input delivers nothing
        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), true);
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
            samples: samples([("input_1", InputSamples::batches(batches))]),
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

    /// The offset is larger than the stretch window, so the batches sent right
    /// after start are withheld until the first chunk that reaches the offset
    /// point.
    #[test]
    fn offset_from_start_delivered_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(100)));

        // the whole stream is sent right after start (queue PTS 100-220ms)
        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        input.send_samples(ms(105), INPUT_BATCH_DURATION);

        // chunks that end before the offset point deliver nothing even though
        // everything is already buffered
        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(60), false);
        assert!(queue.next_audio_batch().is_none());

        // [80, 100) is the first chunk that reaches the offset point: it pops
        // everything within the 80ms stretch window (input PTS below 80ms)
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(80),
                vec![
                    (ms(100), ms(115)),
                    (ms(115), ms(130)),
                    (ms(130), ms(145)),
                    (ms(145), ms(160)),
                    (ms(160), ms(175)),
                    (ms(175), ms(190)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        // one batch per chunk, delivered ~100ms ahead of its PTS
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(190), ms(205))]),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(120), vec![(ms(205), ms(220))]),
        );
        assert!(queue.next_audio_batch().is_none());

        // out of data: empty chunks keep flowing
        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(140), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(160), false);
    }

    /// Sent early, but within the stretch window, so nothing is withheld:
    /// verifies that [0, 20) and [20, 40) stay empty with data already
    /// buffered.
    #[test]
    fn offset_from_start_delivered_slightly_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        input.send_samples(ms(105), INPUT_BATCH_DURATION);

        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(150), ms(165))]),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![(ms(165), ms(180))]),
        );
        assert!(queue.next_audio_batch().is_none());

        // out of data: empty chunks keep flowing (video would repeat the last
        // frame, audio never repeats a batch)

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(120), false);
    }

    #[test]
    fn offset_from_start_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit before

        // an optional input never stalls the queue, so [40, 60) is produced
        // (empty) even though a required input would hold it back
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // no coverage requirement for optional inputs: everything within the
        // stretch window is delivered as soon as [60, 80) is due
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(150), ms(165))]),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_on_time_with_some_batches_behind_mixer_buffer() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit before

        // an optional input never stalls the queue, so [40, 60) is produced
        // (empty) even though a required input would hold it back
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);

        // no coverage requirement for optional inputs: everything within the
        // stretch window is delivered as soon as [60, 80) is due
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![(ms(60), ms(75)), (ms(75), ms(90)), (ms(90), ms(105))],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(80),
                vec![(ms(105), ms(120)), (ms(120), ms(135)), (ms(135), ms(150))],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(150), ms(165))]),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(98));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(60), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // late batches are delivered anyway (video would skip late frames)
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(100),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)), // this is not full 80ms buffer, in required that batch
                                        // would hang
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(120), false);
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

    /// The offset point is more than the stretch window ahead of the queue
    /// start. Unlike `FromStart`, chunks before the offset point still
    /// deliver: each chunk pops the batches within its own stretch window
    /// (queue PTS below chunk end + 80ms), metering the head of the stream
    /// out a batch or two per chunk. Only [0, 20) is empty: its window ends
    /// before the track zero.
    #[test]
    fn offset_pts_after_start_delivered_early() {
        // batches are sent before the queue starts: an optional input pops
        // whatever arrived when a chunk becomes due, so sending right after
        // start would race the first chunk
        let (mut queue, input) =
            create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(103)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        input.send_samples(ms(105), INPUT_BATCH_DURATION);

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(20),
                vec![(ms(103), ms(103 + 15)), (ms(103 + 15), ms(103 + 30))],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(103 + 30), ms(103 + 45))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(103 + 45), ms(103 + 60))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        // the 20ms chunk grid overtakes the 15ms batch grid: two batches fall
        // within the [80, 100) window
        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(80),
                vec![(ms(103 + 60), ms(103 + 75)), (ms(103 + 75), ms(103 + 90))],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(103 + 90), ms(103 + 105))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(120), vec![(ms(103 + 105), ms(103 + 120))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        // out of data: empty chunks keep flowing
        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(140), false);
        assert!(queue.next_audio_batch().is_none());
    }

    /// Sent early, but the offset is within the stretch window: the first
    /// chunk delivers the whole head of the stream (queue PTS below 100ms),
    /// later chunks meter out the rest.
    #[test]
    fn offset_pts_after_start_delivered_slightly_early() {
        // batches are sent before the queue starts: an optional input pops
        // whatever arrived when a chunk becomes due, so sending right after
        // start would race the first chunk
        let (mut queue, input) =
            create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(63)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(63), ms(63 + 15)),
                    (ms(63 + 15), ms(63 + 30)),
                    (ms(63 + 30), ms(63 + 45)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(20), vec![(ms(63 + 45), ms(63 + 60))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(40),
                vec![(ms(63 + 60), ms(63 + 75)), (ms(63 + 75), ms(63 + 90))],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(63 + 90), ms(63 + 105))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        // empty chunks flow because the optional input never stalls the queue
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        sleep(ms(4));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), false);
        assert!(queue.next_audio_batch().is_none());
    }

    /// Like [`offset_pts_after_start_delivered_on_time`], but only the
    /// batches overlapping the next chunk are sent up front; every other
    /// batch is sent as late as possible (just before the first chunk
    /// overlapping its PTS range is produced). Nothing arrives late, so each
    /// batch is still delivered with the first chunk it overlaps.
    #[test]
    fn offset_pts_after_start_delivered_just_in_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        // only the batches overlapping [60, 80): queue PTS 60-90ms
        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);

        sleep(ms(4));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(60), ms(75)), (ms(75), ms(90))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        // queue PTS 90-105ms, first overlaps [80, 100)
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![(ms(90), ms(105))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        // queue PTS 105-120ms, first overlaps [100, 120)
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(105), ms(120))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        // queue PTS 120-150ms, both first overlap [120, 140)
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(120), vec![(ms(120), ms(135)), (ms(135), ms(150))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(140), false);
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(78));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(60), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // batches for queue PTS 60..150ms arrive after those chunks were
        // already produced; they are still delivered with the next chunk
        sleep(ms(4));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(80),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), false);
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_early() {
        // track zero is ~40ms before the queue start: batches that are
        // already in the past (input PTS 0-30ms) get dropped by the pre-start
        // cleanup, like for video
        let (mut queue, input) =
            create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        // no stall for an optional input, [0, 20) delivers what survived the
        // pre-start cleanup
        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![(ms(5), ms(20)), (ms(20), ms(35)), (ms(35), ms(50))],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_late() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        sleep(ms(50) + OFFSET);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // sends are asynchronous: give the relay and the pre-start cleanup a
        // moment to process the batches before the queue starts (the queue
        // ends up starting ~52ms after the track zero)
        sleep(ms(2));
        queue.start();

        // batches 0-45ms (before the effective start point) were dropped by
        // the pre-start cleanup
        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![(ms(60 - 52), ms(75 - 52)), (ms(75 - 52), ms(90 - 52))],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
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
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        // the offset locks to the chunk that first observed a batch ([60, 80));
        // unlike a required input there is no stall waiting for more data
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(60), ms(75)),
                    (ms(75), ms(90)),
                    (ms(90), ms(105)),
                    (ms(105), ms(120)),
                    (ms(120), ms(135)),
                    (ms(135), ms(150)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(150), ms(165))]),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    /// Like [`offset_none_after_start_delivered_on_time`], but only the
    /// batches overlapping the locking chunk are sent up front; every other
    /// batch is sent as late as possible (just before the first chunk
    /// overlapping its PTS range is produced). Nothing arrives late, so each
    /// batch is still delivered with the first chunk it overlaps. The offset
    /// locks to [60, 80) on the chunk grid, so expectations are exact.
    #[test]
    fn offset_none_after_start_delivered_just_in_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        // locks the offset to [60, 80); only the batches overlapping it
        // (queue PTS 60-90ms) are sent
        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);

        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(60), vec![(ms(60), ms(75)), (ms(75), ms(90))]),
        );
        assert!(queue.next_audio_batch().is_none());

        // queue PTS 90-105ms, first overlaps [80, 100)
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(80), vec![(ms(90), ms(105))]),
        );
        assert!(queue.next_audio_batch().is_none());

        // queue PTS 105-120ms, first overlaps [100, 120)
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(100), vec![(ms(105), ms(120))]),
        );
        assert!(queue.next_audio_batch().is_none());

        // queue PTS 120-150ms, both first overlap [120, 140)
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(120), vec![(ms(120), ms(135)), (ms(135), ms(150))]),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(140), false);
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_on_time_first_non_zero() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(5), INPUT_BATCH_DURATION);
        input.send_samples(ms(20), INPUT_BATCH_DURATION);
        input.send_samples(ms(35), INPUT_BATCH_DURATION);
        input.send_samples(ms(50), INPUT_BATCH_DURATION);
        input.send_samples(ms(65), INPUT_BATCH_DURATION);
        input.send_samples(ms(80), INPUT_BATCH_DURATION);

        // none offset maps input 0ms to the chunk start (queue 60ms), so the
        // first batch (input PTS 5ms) starts at queue 65ms
        sleep(ms(4));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(60),
                vec![
                    (ms(65), ms(80)),
                    (ms(80), ms(95)),
                    (ms(95), ms(110)),
                    (ms(110), ms(125)),
                    (ms(125), ms(140)),
                    (ms(140), ms(155)),
                ],
            ),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), false);
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_late_first_packet_early() {
        // the early batch anchors the offset (input 0ms ↔ queue 40ms); the
        // rest of the stream arrives too late but is still delivered with that
        // mapping
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(30));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert!(queue.next_audio_batch().is_none());

        // locks the offset to the next unproduced chunk: input 0ms ↔ queue 40ms
        input.send_samples(ms(0), INPUT_BATCH_DURATION);

        // an optional input doesn't wait for coverage, the early batch is
        // delivered on time
        sleep(ms(12));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(40), ms(55))]),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(60), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);

        // batches for queue PTS 55..100ms are delivered with the [100, 120)
        // chunk, they keep the mapping anchored by the early batch
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(100),
                vec![(ms(55), ms(70)), (ms(70), ms(85)), (ms(85), ms(100))],
            ),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    #[test]
    fn offset_none_before_start() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // batches arrive 25ms before the queue starts; the offset locks at the
        // pre-start cleanup tick (~send time), and pre-start cleanup drops
        // batches whose start PTS is already in the past (0ms and 15ms here)
        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.send_samples(ms(30), INPUT_BATCH_DURATION);
        input.send_samples(ms(45), INPUT_BATCH_DURATION);
        input.send_samples(ms(60), INPUT_BATCH_DURATION);
        input.send_samples(ms(75), INPUT_BATCH_DURATION);

        sleep(ms(25));
        queue.start();

        sleep(ms(1));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(
                ms(0),
                vec![
                    (ms(5), ms(20)),
                    (ms(20), ms(35)),
                    (ms(35), ms(50)),
                    (ms(50), ms(65)),
                ],
            ),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert!(queue.next_audio_batch().is_none());

        input.send_samples(ms(90), INPUT_BATCH_DURATION);
        sleep(ms(20));
        assert_audio_batch_eq_with_tolerance(
            &queue.next_audio_batch().unwrap(),
            &chunk(ms(40), vec![(ms(65), ms(80))]),
            ms(2),
        );
        assert!(queue.next_audio_batch().is_none());
    }

    /// EOS is delivered on the chunk that pops the final batches, and that
    /// chunk is required even for an optional input.
    #[test]
    fn offset_from_start_eos_with_last_batches() {
        let (queue, mut input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(100)));

        input.send_samples(ms(0), INPUT_BATCH_DURATION);
        input.send_samples(ms(15), INPUT_BATCH_DURATION);
        input.end_audio();

        sleep(ms(1));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), false);
        assert!(queue.next_audio_batch().is_none());

        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(60), false);
        assert!(queue.next_audio_batch().is_none());

        // [80, 100) is the first chunk that reaches the offset point; it pops
        // both batches, draining the stream: EOS on the same chunk
        sleep(ms(20));
        assert_audio_batch_eq(
            &queue.next_audio_batch().unwrap(),
            &AudioBatch {
                start_pts: ms(80),
                end_pts: ms(100),
                required: true,
                samples: samples([(
                    "input_1",
                    InputSamples::batches_eos(vec![(ms(100), ms(115)), (ms(115), ms(130))]),
                )]),
            },
        );
        assert!(queue.next_audio_batch().is_none());

        // after EOS the input delivers nothing
        sleep(ms(20));
        assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), false);
    }
}
