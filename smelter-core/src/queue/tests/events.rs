use std::thread::sleep;

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{OFFSET, TestInput, TestQueue, TestQueueOptions, ms};

mod required_input {
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
}
