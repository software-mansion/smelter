use std::{thread::sleep, time::Duration};

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{
    InputFrame, OFFSET, TestInput, TestQueue, TestQueueOptions, VideoBatch,
    assert_empty_video_batch, assert_video_batch_eq, assert_video_batch_eq_with_tolerance, frames,
    ms,
};

fn frame(id: u32, pts: Duration) -> InputFrame {
    InputFrame {
        frame: Some((id, pts)),
        eos: false,
    }
}

mod required_input {
    use super::*;

    /// A batch with a single frame from the required "input_1".
    fn batch(pts: Duration, frame: InputFrame) -> VideoBatch {
        VideoBatch {
            pts,
            required: true,
            frames: frames([("input_1", frame)]),
        }
    }

    /// Create a queue with a single required video-only input ("input_1").
    /// The queue is not started yet, so frames can be sent before start.
    fn create_queue_with_video_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let queue = TestQueue::new(TestQueueOptions::default());
        let input = queue.add_input(
            "input_1",
            QueueInputOptions {
                required: true,
                ..Default::default()
            },
            QueueTrackOptions {
                video: true,
                audio: false,
                offset,
            },
        );
        (queue, input)
    }

    /// Like [`create_queue_with_video_input`], but desync the clocks and start
    /// the queue.
    fn start_queue_with_video_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let (mut queue, input) = create_queue_with_video_input(offset);

        // desync regular clock from queue clock
        sleep(OFFSET);

        queue.start();
        (queue, input)
    }

    #[test]
    fn offset_from_start_delivered_early() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));
        input.send_frame(ms(75));

        sleep(ms(1));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
        );
        assert!(queue.next_video_batch().is_none());

        // frame 3 is skipped

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(120), frame(4, ms(120))),
        );

        sleep(ms(20));
        assert!(queue.next_video_batch().is_none());
        // frame 5 is not generated because there was not frame 6 nothing after 75ms
    }

    #[test]
    fn offset_from_start_delivered_on_time() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit before

        // no frames will be returned until 60ms passes, just empty batches
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        // no frame because send_frame(ms(45)) was not sent yet
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(200));
        // receiving empty batches before input offset start, but not after offset
        // because at that point we expect required input
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        // no frames will be returned until 60ms passes, just empty batches
        sleep(ms(1));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        // no frame because send_frame(ms(45)) was not sent yet
        assert!(queue.next_video_batch().is_none());
    }

    //
    // Pts offsets are relative to the sync point (queue creation), summaries
    // to the queue start: `Pts(OFFSET + d)` places the track zero ~d after
    // start, `Pts(OFFSET - d)` ~d before start.
    //

    #[test]
    fn offset_pts_after_start_delivered_early() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(1));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        // we don't know what will be first frame pts, so batches 0, 20, 40
        // can't be produced at this point
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));

        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        // no frame because send_frame(ms(45)) was not sent yet
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time_with_first_packet_early() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));

        // TODO: this packet should not be necessary so the empty batches
        // bellow are sent early
        input.send_frame(ms(15));

        sleep(ms(58));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(30));

        sleep(ms(4));

        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        // no frame because send_frame(ms(45)) was not sent yet
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_late() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(200));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(1));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );
        // no frame because send_frame(ms(45)) was not sent yet
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_late_first_packet_early() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));
        // TODO: this packet should not be necessary so the empty batches
        // bellow are sent early
        input.send_frame(ms(15));

        sleep(ms(200));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(30));
        input.send_frame(ms(45));
        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
            ms(2),
        );
        // no frame because send_frame(ms(60)) was not sent yet
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_early() {
        // track zero is ~60ms before the queue start: frames with PTS below
        // ~60ms are already late at start and get dropped
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        // all frames arrive before the queue starts
        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        // frames 0 and 1 are dropped, frame 2 (input PTS 30ms) is due at start (would -10 ms but
        // duration is positive)
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(2, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        // a newer frame is needed before frame 4 can be returned
        input.send_frame(ms(75));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(4, ms(20))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_on_time_not_aligned_before() {
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(0, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(75));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(2, ms(20))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_on_time_not_aligned_after() {
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_frame(ms(45));
        input.send_frame(ms(60));

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        // 45ms is newer than 40ms and we always take newer
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(75));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(1, ms(20))), // frame 0 is dropped
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_on_time_aligned() {
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_frame(ms(40));
        input.send_frame(ms(60));

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(0, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(100));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(1, ms(20))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_late() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET));

        sleep(ms(50) + OFFSET);
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));
        input.send_frame(ms(75));
        input.send_frame(ms(90));
        input.send_frame(ms(105));

        queue.start();

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(3, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(4, ms(10))),
            ms(2),
        );
        // no frame newer than input PTS 100ms was sent yet
        assert!(queue.next_video_batch().is_none());
    }

    //
    // None offset: the offset is not known upfront, it locks to the queue PTS
    // of the batch that first observes a frame. Until then a required input
    // with a `None` offset reports ready (producing empty batches) instead of
    // stalling like a `Pts` offset.
    //

    #[test]
    fn offset_none_after_start_delivered_on_time() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        // input reports ready with no data: empty batches until a frame arrives
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));
        // the offset locks to the batch that first observed a frame (60ms), so
        // frame 0 (input PTS 0) plays there
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        // frame 2 needs a newer frame before it can be returned
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_on_time_first_non_zero() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        // input reports ready with no data: empty batches until a frame arrives
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(5));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));
        // none offset will assign input 0ms value to first queue pts that was
        // observed, so input 0ms will mean queue 60ms
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(60));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        // frame 2 needs a newer frame before it can be returned
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_late_with_gap() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        // empty batches keep flowing until the first frame arrives
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(5));
        sleep(ms(4));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(25));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        sleep(ms(2));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(65))), // frame 2 would be closer but we always take older
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(3, ms(90))),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_none_before_start() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // frames arrive 20ms before the queue starts; the offset locks at the
        // pre-start cleanup tick (~start), so the stream plays aligned from 0
        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(65)); // 60 would cause race condition in last assert

        sleep(ms(20));
        queue.start();

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(1, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(2, ms(10))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(75));

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(40), frame(3, ms(25))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }
}

mod optional_input {
    use super::*;

    /// A batch with a single frame from the required "input_1".
    fn batch(pts: Duration, frame: InputFrame) -> VideoBatch {
        VideoBatch {
            pts,
            required: false,
            frames: frames([("input_1", frame)]),
        }
    }

    /// Create a queue with a single required video-only input ("input_1").
    /// The queue is not started yet, so frames can be sent before start.
    fn create_queue_with_video_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let queue = TestQueue::new(TestQueueOptions::default());
        let input = queue.add_input(
            "input_1",
            QueueInputOptions {
                required: false,
                ..Default::default()
            },
            QueueTrackOptions {
                video: true,
                audio: false,
                offset,
            },
        );
        (queue, input)
    }

    /// Like [`create_queue_with_video_input`], but desync the clocks and start
    /// the queue.
    fn start_queue_with_video_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let (mut queue, input) = create_queue_with_video_input(offset);

        // desync regular clock from queue clock
        sleep(OFFSET);

        queue.start();
        (queue, input)
    }

    #[test]
    fn offset_from_start_delivered_early() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));
        input.send_frame(ms(75));

        sleep(ms(1));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
        );
        assert!(queue.next_video_batch().is_none());

        // frame 3 is skipped

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(120), frame(4, ms(120))),
        );

        // queue keeps producing frameset with last frame

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(140), frame(5, ms(135))),
        );

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(160), frame(5, ms(135))),
        );

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(180), frame(5, ms(135))),
        );
    }

    #[test]
    fn offset_from_start_delivered_on_time() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit before

        // no frames will be returned until 60ms passes, just empty batches
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(98));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(60));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(80));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));
        input.send_frame(ms(75));
        // no frames will be returned until 60ms passes, just empty batches
        sleep(ms(4));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(120), frame(4, ms(120))),
        );
        assert!(queue.next_video_batch().is_none());
    }

    //
    // Pts offsets are relative to the sync point (queue creation), summaries
    // to the queue start: `Pts(OFFSET + d)` places the track zero ~d after
    // start, `Pts(OFFSET - d)` ~d before start.
    //

    #[test]
    fn offset_pts_after_start_delivered_early() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(1));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));

        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        // send_frame(ms(45)) was not called yet but input is not required
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
            ms(2),
        );
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time_with_first_packet_early() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));

        sleep(ms(58));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));

        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        // send_frame(ms(45)) was not called yet but input is not required
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
            ms(2),
        );
    }

    #[test]
    fn offset_pts_after_start_delivered_late() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(78));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(60));

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));

        sleep(ms(4));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
            ms(2),
        );
    }

    #[test]
    fn offset_pts_after_start_delivered_late_first_packet_early() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));

        sleep(ms(78));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        sleep(ms(4));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
            ms(2),
        );

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
            ms(2),
        );
    }

    #[test]
    fn offset_pts_before_start_delivered_early() {
        // track zero is ~60ms before the queue start: frames with PTS below
        // ~60ms are already late at start and get dropped
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        // all frames arrive before the queue starts
        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        // frames 0 and 1 are dropped, frame 2 (input PTS 30ms) is due at start (would -10 ms but
        // duration is positive)
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(2, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        // a newer frame is needed before frame 4 can be returned
        input.send_frame(ms(75));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(4, ms(20))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_on_time_not_aligned_before() {
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(0, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(75));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(2, ms(20))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_on_time_not_aligned_after() {
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_frame(ms(45));
        input.send_frame(ms(60));

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        // 45ms is newer than 40ms and we always take newer
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(75));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(1, ms(20))), // frame 0 is dropped
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_on_time_aligned() {
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(40)));

        input.send_frame(ms(40));
        input.send_frame(ms(60));

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(0, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(100));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(1, ms(20))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_late() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET));

        sleep(ms(50) + OFFSET);
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(60));
        input.send_frame(ms(75));
        input.send_frame(ms(90));
        input.send_frame(ms(105));

        queue.start();

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(3, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(4, ms(10))),
            ms(2),
        );

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(40), frame(6, ms(40))),
            ms(2),
        );
    }

    //
    // None offset: the offset is not known upfront, it locks to the queue PTS
    // of the batch that first observes a frame. Until then a required input
    // with a `None` offset reports ready (producing empty batches) instead of
    // stalling like a `Pts` offset.
    //

    #[test]
    fn offset_none_after_start_delivered_on_time() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        // input reports ready with no data: empty batches until a frame arrives
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));
        // the offset locks to the batch that first observed a frame (60ms), so
        // frame 0 (input PTS 0) plays there
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_on_time_first_non_zero() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        // input reports ready with no data: empty batches until a frame arrives
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(5));
        input.send_frame(ms(15));
        input.send_frame(ms(30));

        sleep(ms(4));
        // none offset will assign input 0ms value to first queue pts that was
        // observed, so input 0ms will mean queue 60ms
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(60));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(75))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(2, ms(90))),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_none_after_start_delivered_late_with_gap() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        // empty batches keep flowing until the first frame arrives
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(5));
        sleep(ms(4));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(25));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        sleep(ms(2));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(65))), // frame 2 would be closer but we always take older
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(100), frame(3, ms(90))),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_none_before_start() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // frames arrive 20ms before the queue starts; the offset locks at the
        // pre-start cleanup tick (~start), so the stream plays aligned from 0
        input.send_frame(ms(0));
        input.send_frame(ms(15));
        input.send_frame(ms(30));
        input.send_frame(ms(45));
        input.send_frame(ms(65)); // 60 would cause race condition in last assert

        sleep(ms(20));
        queue.start();

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(1, ms(0))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(2, ms(10))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(75));

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(40), frame(3, ms(25))),
            ms(2),
        );
        assert!(queue.next_video_batch().is_none());
    }
}
