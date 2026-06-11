use std::{thread::sleep, time::Duration};

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{
    InputFrame, OFFSET, TestInput, TestQueue, TestQueueOptions, VideoBatch,
    assert_empty_video_batch, assert_video_batch_eq, frames, ms,
};

fn frame(id: u32, pts: Duration) -> InputFrame {
    InputFrame::Frame { id, pts }
}

fn batch(pts: Duration, frame: InputFrame) -> VideoBatch {
    VideoBatch {
        pts,
        required: true,
        frames: frames([("input_1", frame)]),
    }
}

mod required_input {
    use crate::queue::tests::harness::assert_video_batch_eq_with_tolerance;

    use super::*;

    /// Create a queue with a single required video-only input ("input_1"),
    /// desync the clocks and start the queue.
    fn start_queue_with_video_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let mut queue = TestQueue::new(TestQueueOptions::default());
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

        // desync regular clock from queue clock
        sleep(OFFSET);

        queue.start();
        (queue, input)
    }

    #[test]
    fn offset_from_start_delivered_early() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.send_frame(ms(40));

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
            &batch(ms(80), frame(1, ms(80))),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_on_time() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit before
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.send_frame(ms(40));

        // no frames will be returned until 60ms passes, just empty batches
        sleep(ms(3));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(80))),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_from_start_delivered_late() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(200));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.send_frame(ms(40));
        // no frames will be returned until 60ms passes, just empty batches
        sleep(ms(1));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(80))),
        );
        // no frame because send_frame(ms(60)) was not sent yet
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
        input.send_frame(ms(20));
        input.send_frame(ms(40));

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
            ms(1),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(80))),
            ms(1),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_on_time() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58)); // a bit before
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.send_frame(ms(40));

        sleep(ms(3));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(1),
        );
        assert!(queue.next_video_batch().is_none());

        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(80))),
            ms(1),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_after_start_delivered_late() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(200));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20));
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.send_frame(ms(40));

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
            ms(1),
        );
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(80), frame(1, ms(80))),
            ms(1),
        );
        // no frame because send_frame(ms(60)) was not sent yet
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_early() {
        // track zero is ~60ms before the queue start: frames with PTS below
        // ~60ms are already late at start and get dropped
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.send_frame(ms(40));
        input.send_frame(ms(60));
        input.send_frame(ms(80));

        sleep(ms(1));
        // frames 0-2 are dropped, frame 3 (input PTS 60ms) is due at start
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(3, ms(0))),
            ms(1),
        );
        assert!(queue.next_video_batch().is_none());

        // a newer frame is needed before frame 4 can be returned
        input.send_frame(ms(100));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(4, ms(20))),
            ms(1),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_on_time() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(60)));

        // send only the frames that are still due (input PTS >= ~60ms)
        input.send_frame(ms(60));
        input.send_frame(ms(80));

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(0, ms(0))),
            ms(1),
        );
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(100));
        sleep(ms(20));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(1, ms(20))),
            ms(1),
        );
        assert!(queue.next_video_batch().is_none());
    }

    #[test]
    fn offset_pts_before_start_delivered_late() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET - ms(60)));

        // the track offset already passed at start, so there is no pre-start
        // window: every batch waits for the required input
        sleep(ms(100));
        assert!(queue.next_video_batch().is_none());

        input.send_frame(ms(60));
        input.send_frame(ms(80));
        input.send_frame(ms(100));

        sleep(ms(1));
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(0), frame(0, ms(0))),
            ms(1),
        );
        assert_video_batch_eq_with_tolerance(
            &queue.next_video_batch().unwrap(),
            &batch(ms(20), frame(1, ms(20))),
            ms(1),
        );
        // no frame newer than input PTS 100ms was sent yet
        assert!(queue.next_video_batch().is_none());
    }
}

mod optional_input {
    use super::*;

    /// Create a queue with a single optional video-only input ("input_1"),
    /// desync the clocks and start the queue.
    fn start_queue_with_video_input(offset: QueueTrackOffset) -> (TestQueue, TestInput) {
        let mut queue = TestQueue::new(TestQueueOptions::default());
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

        // desync regular clock from queue clock
        sleep(OFFSET);

        queue.start();
        (queue, input)
    }

    #[test]
    fn offset_from_start_event_delivered_early() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_frame(ms(0));
        // event should be delivered immediately
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        input.send_frame(ms(20));
        input.send_frame(ms(40));
        input.send_frame(ms(60));
        // no frames will be returned until 60ms passes
        sleep(ms(1));
        assert!(queue.next_video_batch().is_none());

        sleep(ms(60));
        assert_video_batch_eq(
            &queue.next_video_batch().unwrap(),
            &batch(ms(60), frame(0, ms(60))),
        );
    }
}
