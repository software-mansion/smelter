use std::thread::sleep;

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{BATCH_DURATION, OFFSET, TestInput, TestQueue, TestQueueOptions, ms};

// Event tests cover each offset configuration, asserting the
// delivered/playing/eos lifecycle:
// - `delivered` fires on the first tick the receiver has a buffered frame
//   (before the real-time deadline). A single frame never reaches output, so it
//   isolates `delivered`.
// - `playing` fires when the first real frame is pushed to a batch.
// - `eos` fires on the batch that drains the stream, together with the last
//   frame.
//
// Variants that only change which frame lands in which batch (first packet
// early, aligned/unaligned offsets, gaps) are not repeated per offset: they
// produce an identical event sequence.
//
// The `*_audio_input` modules mirror the same matrix for audio; timing
// differences are explained in their module docs.

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

        // frames play in the 60ms and 80ms batches; the 80ms batch drains the
        // stream, so EOS is emitted together with the last frame
        sleep(ms(80));
        queue.expect_events(&[input.video_playing_event(), input.video_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_from_start_event_eos_without_frames() {
        let (mut queue, mut input) =
            create_queue_with_video_input(QueueTrackOffset::FromStart(ms(60)));

        // desync regular clock from queue clock
        sleep(OFFSET);

        // the track ends before the queue starts, without a single frame; the
        // pre-start cleanup tick observes the closed track
        input.end_video();
        sleep(ms(1));
        queue.expect_events(&[input.video_delivered_event()]);

        // EOS is emitted with the first batch even though the offset never
        // resolved and no frame was ever delivered
        queue.start();
        sleep(ms(1));
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

        // frames play in the 60ms and 80ms batches; the 80ms batch drains the
        // stream, so EOS is emitted together with the last frame
        sleep(ms(80));
        queue.expect_events(&[input.video_playing_event(), input.video_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
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

        // EOS is emitted together with the last frame (20ms batch)
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

        // the second frame plays in the 80ms batch, which drains the stream:
        // EOS is emitted together with the last frame
        sleep(ms(20));
        queue.expect_events(&[input.video_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
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

        // EOS is emitted together with the last frame (20ms batch)
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

        // the 80ms batch drains the stream: EOS together with the last frame
        sleep(ms(80));
        queue.expect_events(&[input.video_playing_event(), input.video_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
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

        // the 80ms batch drains the stream: EOS together with the last frame
        sleep(ms(80));
        queue.expect_events(&[input.video_playing_event(), input.video_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
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

        // EOS is emitted together with the last frame (20ms batch)
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

        // the second frame plays in the 80ms batch, which drains the stream:
        // EOS is emitted together with the last frame
        sleep(ms(20));
        queue.expect_events(&[input.video_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
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

        // EOS is emitted together with the last frame (20ms batch)
        sleep(ms(40));
        queue.expect_events(&[input.video_eos_event()]);
    }
}

mod required_audio_input {
    //! Audio variants of the [`required_input`] scenarios. The
    //! delivered/playing/eos lifecycle is the same, but the timing differs:
    //! - `playing` fires when the first batch is popped into a chunk. Batches
    //!   are popped up to chunk end + 80ms (mixer stretch buffer) ahead, so
    //!   for a `Pts` offset the first chunk pops everything buffered and for
    //!   `FromStart` the chunk just before the offset point does.
    //! - a required input pops nothing until it is buffered ~100ms ahead of
    //!   the produced chunk, so `playing` tests send more data than their
    //!   video counterparts. After EOS the input is always ready, so `eos`
    //!   tests get by with two batches.
    //! - the pre-start cleanup drops old batches for `Pts` and `None`
    //!   offsets, which can leave nothing to play at start.

    use super::*;

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

    //
    // FromStart offset
    //

    #[test]
    fn offset_from_start_event_delivered_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58)); // a bit less
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(100));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_playing() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        // enough data for the required input to report ready (80ms)
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // chunks entirely before the 60ms offset don't include the input
        sleep(ms(20));
        queue.expect_events(&[]);

        // playing fires with the [40, 60) chunk: the first chunk not entirely
        // before the offset point pops everything in the stretch window
        // (video plays at 60ms)
        sleep(ms(20));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_from_start_event_eos() {
        let (queue, mut input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // after EOS the input is always ready; both batches are popped by the
        // [40, 60) chunk, which drains the stream: EOS on the same chunk
        sleep(ms(40));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_from_start_event_eos_without_samples() {
        let (mut queue, mut input) =
            create_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        // desync regular clock from queue clock
        sleep(OFFSET);

        // the track ends before the queue starts, without a single batch; the
        // pre-start cleanup tick observes the closed track
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // EOS is emitted with the first chunk even though the offset never
        // resolved and no samples were ever delivered
        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_eos_event()]);
    }

    //
    // Pts offset, resolved after start. The offset is fixed up front, so
    // `delivered` still fires as soon as the first batch is buffered.
    //

    #[test]
    fn offset_pts_after_start_event_delivered_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(100));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_playing() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);

        // the first chunk pops everything below queue PTS 100ms (the batches
        // at input 0ms and 20ms): unlike video, playing fires right away
        // instead of at the 60ms offset point
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event(), input.audio_playing_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_eos() {
        let (queue, mut input) =
            start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();

        // after EOS the input is always ready, the first chunk pops both
        // batches immediately, draining the stream: EOS on the same chunk
        sleep(ms(1));
        queue.expect_events(&[
            input.audio_delivered_event(),
            input.audio_playing_event(),
            input.audio_eos_event(),
        ]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    //
    // Pts offset, batches received before start. `delivered` is emitted during
    // the pre-start cleanup tick, so it is observable before `start`.
    // `Pts(OFFSET)` maps input PTS 1:1 onto queue PTS; the cleanup also drops
    // batches whose start PTS is already in the past.
    //

    #[test]
    fn offset_pts_before_start_event_delivered() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        // single batch: delivered fires during pre-start cleanup, but the
        // batch is dropped on the same tick (its start PTS is already in the
        // past), so nothing can play after start
        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_pts_before_start_event_playing() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        // batch 0ms gets dropped by the pre-start cleanup; the rest covers
        // the ~100ms the required input needs to report ready
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);
        input.send_samples(ms(100), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // batches play with the first chunk
        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_pts_before_start_event_eos() {
        let (mut queue, mut input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // batch 0ms was dropped before start, batch 20ms plays with the first
        // chunk (after EOS readiness doesn't need the 100ms of coverage),
        // which drains the stream: EOS on the same chunk
        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    //
    // None offset, resolved after start. The offset locks to the chunk that
    // first observes a batch.
    //

    #[test]
    fn offset_none_after_start_event_delivered() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        // a required None input reports ready (empty chunks), no events yet
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_none_after_start_event_playing() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        // offset locks to the [60, 80) chunk, which needs the input buffered
        // up to 100ms before it pops anything
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // playing is emitted when the [60, 80) chunk becomes due
        sleep(ms(4));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_none_after_start_event_eos() {
        let (queue, mut input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // both batches are popped by the [60, 80) chunk, which drains the
        // stream: EOS on the same chunk
        sleep(ms(4));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    //
    // None offset, batches received before start. The offset locks at the
    // pre-start cleanup tick; the same cleanup drops batches whose start PTS
    // is already in the past.
    //

    #[test]
    fn offset_none_before_start_event_delivered() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // the batch is dropped right after the offset locks, so nothing can
        // play after start
        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_none_before_start_event_playing() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // batch 0ms gets dropped by the pre-start cleanup; the rest covers
        // the ~100ms the required input needs to report ready
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.send_samples(ms(40), BATCH_DURATION);
        input.send_samples(ms(60), BATCH_DURATION);
        input.send_samples(ms(80), BATCH_DURATION);
        input.send_samples(ms(100), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_none_before_start_event_eos() {
        let (mut queue, mut input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // batch 0ms was dropped before start, batch 20ms plays with the first
        // chunk, which drains the stream: EOS on the same chunk
        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }
}

mod optional_audio_input {
    //! Same offset configurations as [`required_audio_input`], but the input
    //! is optional. An optional input never stalls the queue and doesn't wait
    //! for coverage: any buffered batch is popped as soon as the next chunk is
    //! due. That makes `playing` fire much earlier than for a required input,
    //! and sending a batch right after start would race the first chunk pop —
    //! tests either send before start or mid-chunk to stay deterministic.

    use super::*;

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

    //
    // FromStart offset. Chunks entirely before the offset point never pop, so
    // an early batch doesn't race the pop like it does for a `Pts` offset.
    //

    #[test]
    fn offset_from_start_event_delivered_early() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        sleep(ms(101));
        // +1ms so the chunk covering 100ms is produced before the batch
        // arrives; the batch is then delivered with the 120ms chunk, after
        // the assert below
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(2));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_from_start_event_playing() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        // one batch is enough: an optional input doesn't wait for coverage
        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // chunks entirely before the 60ms offset don't include the input
        sleep(ms(20));
        queue.expect_events(&[]);

        // playing fires with the [40, 60) chunk (video plays at 60ms)
        sleep(ms(20));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_from_start_event_eos() {
        let (queue, mut input) = start_queue_with_audio_input(QueueTrackOffset::FromStart(ms(60)));

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // both batches are popped by the [40, 60) chunk, which drains the
        // stream: EOS on the same chunk
        sleep(ms(40));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    //
    // Pts offset, resolved after start. A batch sent right after start races
    // the first chunk pop (playing could fire on the same tick or a chunk
    // later), so these tests place the sends deterministically instead.
    //

    #[test]
    fn offset_pts_after_start_event_delivered_early() {
        let (mut queue, input) =
            create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        // the batch arrives before start: delivered fires during the
        // pre-start cleanup tick (the offset is in the future, nothing is
        // dropped)
        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // desync regular clock from queue clock
        sleep(OFFSET);
        queue.start();

        // the first chunk pops the batch right away
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_delivered_on_time() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        // the batch will be popped by the [60, 80) chunk, after this assert
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_delivered_late() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(101));
        // +1ms so the chunk covering 100ms is produced before the batch
        // arrives; the batch is then delivered with the 120ms chunk, after
        // the assert below
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(2));
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_playing() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        // send mid-chunk so the pop timing is deterministic
        sleep(ms(30));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // the [40, 60) chunk pops the batch ~20ms before its PTS (60ms)
        sleep(ms(12));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_pts_after_start_event_eos() {
        let (queue, mut input) =
            start_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET + ms(60)));

        sleep(ms(30));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // both batches are popped by the [40, 60) chunk, which drains the
        // stream: EOS on the same chunk
        sleep(ms(12));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    //
    // Pts offset, batches received before start.
    //

    #[test]
    fn offset_pts_before_start_event_delivered() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        // the batch is dropped by the pre-start cleanup right after delivery
        // (video keeps the frame and needs a PTS shift to avoid playing)
        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_pts_before_start_event_playing() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        // batch 0ms gets dropped by the pre-start cleanup, batch 20ms plays
        // with the first chunk
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_pts_before_start_event_eos() {
        let (mut queue, mut input) = create_queue_with_audio_input(QueueTrackOffset::Pts(OFFSET));

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // the remaining batch plays with the first chunk, which drains the
        // stream: EOS on the same chunk
        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    //
    // None offset, resolved after start.
    //

    #[test]
    fn offset_none_after_start_event_delivered() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        // the offset locks to the [60, 80) chunk, which pops the batch after
        // this assert
        queue.expect_events(&[input.audio_delivered_event()]);
    }

    #[test]
    fn offset_none_after_start_event_playing() {
        let (queue, input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // playing is emitted when the [60, 80) chunk (which the offset locked
        // to) becomes due
        sleep(ms(4));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_none_after_start_event_eos() {
        let (queue, mut input) = start_queue_with_audio_input(QueueTrackOffset::None);

        sleep(ms(58));
        queue.expect_events(&[]);

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // both batches are popped by the [60, 80) chunk, which drains the
        // stream: EOS on the same chunk
        sleep(ms(4));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }

    //
    // None offset, batches received before start.
    //

    #[test]
    fn offset_none_before_start_event_delivered() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_samples(ms(0), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        queue.start();
        sleep(ms(1));
        // unlike video (where the frame plays on the first output), the batch
        // was already dropped by the pre-start cleanup
        queue.expect_events(&[]);
    }

    #[test]
    fn offset_none_before_start_event_playing() {
        let (mut queue, input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        // batch 0ms gets dropped by the pre-start cleanup, batch 20ms plays
        // with the first chunk
        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event()]);
    }

    #[test]
    fn offset_none_before_start_event_eos() {
        let (mut queue, mut input) = create_queue_with_audio_input(QueueTrackOffset::None);

        // desync regular clock from queue clock
        sleep(OFFSET);

        input.send_samples(ms(0), BATCH_DURATION);
        input.send_samples(ms(20), BATCH_DURATION);
        input.end_audio();
        sleep(ms(1));
        queue.expect_events(&[input.audio_delivered_event()]);

        // the remaining batch plays with the first chunk, which drains the
        // stream: EOS on the same chunk
        queue.start();
        sleep(ms(1));
        queue.expect_events(&[input.audio_playing_event(), input.audio_eos_event()]);

        sleep(ms(20));
        queue.expect_events(&[]);
    }
}
