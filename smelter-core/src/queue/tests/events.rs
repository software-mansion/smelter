use std::thread::sleep;

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{OFFSET, TestInput, TestQueue, TestQueueOptions, ms};

// Event tests cover each offset configuration, asserting the
// delivered/playing/eos lifecycle:
// - `delivered` fires on the first tick the receiver has a buffered frame
//   (before the real-time deadline). A single frame never reaches output, so it
//   isolates `delivered`.
// - `playing` fires when the first real frame is pushed to a batch.
// - `eos` fires on the first batch after the stream drains.
//
// Variants that only change which frame lands in which batch (first packet
// early, aligned/unaligned offsets, gaps) are not repeated per offset: they
// produce an identical event sequence.

mod required_input {
    use super::*;

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

    //
    // FromStart offset
    //

    #[test]
    fn offset_from_start_event_delivered_early() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_delivered_on_time() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit less
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_delivered_late() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(100));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_playing() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        // batches before the 60ms offset don't include the input
        sleep(ms(40));
        queue.expect_events(&[]);

        // playing is emitted when the first frame reaches the output (60ms batch)
        sleep(ms(20));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_from_start_event_eos() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        // frames play in the 60ms and 80ms batches
        sleep(ms(80));
        queue.expect_events(&[input.video_playing_event()]);

        // EOS is emitted with the first batch after the stream runs dry (100ms)
        sleep(ms(20));
        queue.expect_events(&[input.video_eos_event()]);
    }

    //
    // Pts offset, resolved after start. The offset is fixed up front, so
    // `delivered` still fires as soon as the first frame is buffered.
    //

    #[test]
    fn offset_pts_after_start_event_delivered_early() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_delivered_on_time() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_delivered_late() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(100));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_playing() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        // empty batches before the 60ms offset don't include the input
        sleep(ms(40));
        queue.expect_events(&[]);

        // playing is emitted when the first frame reaches the output (60ms batch)
        sleep(ms(20));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_eos() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        // frames play in the 60ms and 80ms batches
        sleep(ms(80));
        queue.expect_events(&[input.video_playing_event()]);

        // EOS is emitted with the first batch after the stream runs dry (100ms)
        sleep(ms(20));
        queue.expect_events(&[input.video_eos_event()]);
    }

    //
    // Pts offset, frames received before start. `delivered` is emitted during
    // the pre-start cleanup tick, so it is observable before `start`.
    // `Pts(OFFSET)` maps input PTS 1:1 onto queue PTS.
    //

    #[test]
    fn offset_pts_before_start_event_delivered() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        // single frame: delivered fires during pre-start cleanup, but the frame
        // never reaches output (needs a newer frame)
        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        // no playing because we don't have second send_frame
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_pts_before_start_event_playing() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        // frames play aligned from the start (0ms batch)
        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_pts_before_start_event_eos() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        // frames play in the 0ms and 20ms batches
        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.video_playing_event()]);

        // EOS is emitted with the first batch after the stream runs dry (40ms)
        sleep(ms(40));
        queue.expect_events(&[input.video_eos_event()]);
    }

    //
    // None offset, resolved after start. The offset locks to the queue PTS of
    // the batch that first observes a frame.
    //

    #[test]
    fn offset_none_after_start_event_delivered() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        // a required None input reports ready (empty batches), no events yet
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_none_after_start_event_playing() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        // offset locks to the batch that first observed a frame (60ms)
        queue.expect_events(&[input.video_delivered_event()]);

        // playing is emitted when the first frame reaches the output (60ms batch)
        sleep(ms(4));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_none_after_start_event_eos() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        // first frame plays in the 60ms batch
        sleep(ms(4));
        queue.expect_events(&[input.video_playing_event()]);

        // second frame plays in the 80ms batch, no new event
        sleep(ms(20));
        queue.expect_events(&[]);

        // EOS is emitted with the first batch after the stream runs dry (100ms)
        sleep(ms(20));
        queue.expect_events(&[input.video_eos_event()]);
    }

    //
    // None offset, frames received before start. The offset locks at the
    // pre-start cleanup tick, so the stream plays aligned from the start.
    //

    #[test]
    fn offset_none_before_start_event_delivered() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        // no second send_frame so no playing event
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_none_before_start_event_playing() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_none_before_start_event_eos() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.video_playing_event()]);

        // EOS is emitted with the first batch after the stream runs dry (40ms)
        sleep(ms(40));
        queue.expect_events(&[input.video_eos_event()]);
    }
}

mod optional_input {
    //! Same offset configurations as `required_input`, but the input is
    //! optional. The delivered/playing/eos lifecycle is the same; the
    //! difference is that an optional input never stalls the queue.

    use super::*;

    /// Create a queue with a single optional video-only input ("input_1").
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

    //
    // FromStart offset. An optional input never stalls the queue, but the
    // delivered/playing/eos lifecycle is the same as for a required input.
    //

    #[test]
    fn offset_from_start_event_delivered_early() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_delivered_on_time() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_delivered_late() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(101));
        // +1ms to go out of sync with output frames, so playing event is not sent
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(2));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_playing() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        sleep(ms(40));
        queue.expect_events(&[]);

        sleep(ms(20));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_from_start_event_eos() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        sleep(ms(80));
        queue.expect_events(&[input.video_playing_event()]);

        sleep(ms(20));
        queue.expect_events(&[input.video_eos_event()]);
    }

    //
    // Pts offset, resolved after start.
    //

    #[test]
    fn offset_pts_after_start_event_delivered_early() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_delivered_on_time() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_delivered_late() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(101));
        // +1 to go out of sync with output batches, so playing even is not sent
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_playing() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        sleep(ms(40));
        queue.expect_events(&[]);

        sleep(ms(20));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_eos() {
        let (queue, mut input) =
            start_queue_with_video_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        sleep(ms(80));
        queue.expect_events(&[input.video_playing_event()]);

        sleep(ms(20));
        queue.expect_events(&[input.video_eos_event()]);
    }

    //
    // Pts offset, frames received before start.
    //

    #[test]
    fn offset_pts_before_start_event_delivered() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(5)); // shift into the future to avoid playing event
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_pts_before_start_event_playing() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_pts_before_start_event_eos() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.video_playing_event()]);

        sleep(ms(40));
        queue.expect_events(&[input.video_eos_event()]);
    }

    //
    // None offset, resolved after start.
    //

    #[test]
    fn offset_none_after_start_event_delivered() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);
    }

    #[test]
    fn offset_none_after_start_event_playing() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        sleep(ms(3));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_none_after_start_event_eos() {
        let (queue, mut input) = start_queue_with_video_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        sleep(ms(3));
        queue.expect_events(&[input.video_playing_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);

        sleep(ms(20));
        queue.expect_events(&[input.video_eos_event()]);
    }

    //
    // None offset, frames received before start.
    //

    #[test]
    fn offset_none_before_start_event_delivered() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        // input optional, so event is sent on first output frame
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_none_before_start_event_playing() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.video_playing_event()]);
    }

    #[test]
    fn offset_none_before_start_event_eos() {
        let (mut queue, mut input) = create_queue_with_video_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_frame(ms(0));
        input.send_frame(ms(20));
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.video_playing_event()]);

        sleep(ms(40));
        queue.expect_events(&[input.video_eos_event()]);
    }
}
