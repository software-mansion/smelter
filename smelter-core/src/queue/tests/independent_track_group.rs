//! Independent track groups: tracks are added to a group after it was queued, and a track of
//! the same kind as a running one starts once the running one delivered EOS.
//!
//! Tests use `FromStart` offsets, so expected PTS are exact.

use std::{thread::sleep, time::Duration};

use crate::{
    Timestamp,
    queue::{QueueInputOptions, QueueTrackGroupOffset},
};

use super::harness::{
    AudioBatch, BATCH_DURATION, INPUT_BATCH_DURATION, InputFrame, InputSamples, OFFSET, TestInput,
    TestQueue, TestQueueOptions, VideoBatch, assert_audio_batch_eq, assert_empty_audio_batch,
    assert_empty_video_batch, assert_video_batch_eq, frames, ms, samples,
};

fn frame(id: u32, pts: Timestamp) -> InputFrame {
    InputFrame::frame(id, pts)
}

fn frame_eos(id: u32, pts: Timestamp) -> InputFrame {
    InputFrame::frame_eos(id, pts)
}

/// A batch with a single frame from the required "input_1".
fn batch(pts: Timestamp, frame: InputFrame) -> VideoBatch {
    VideoBatch {
        pts,
        required: true,
        frames: frames([("input_1", frame)]),
    }
}

/// A chunk with sample batches from the required "input_1".
fn chunk(start_pts: Timestamp, batches: Vec<(u32, Timestamp, Timestamp)>) -> AudioBatch {
    AudioBatch {
        start_pts,
        end_pts: start_pts + BATCH_DURATION,
        required: true,
        samples: samples([("input_1", InputSamples::batches(batches))]),
    }
}

/// Create a queue with a single required input ("input_1") whose first track group is an
/// independent group without tracks. The queue is not started yet.
fn create_queue_with_input(offset: QueueTrackGroupOffset) -> (TestQueue, TestInput) {
    let queue = TestQueue::new(TestQueueOptions::default());
    let input = queue.add_input_with_independent_track_group(
        "input_1",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
        offset,
    );
    (queue, input)
}

/// Before a track is added the input is not part of the batch. A track added later uses the
/// group's `FromStart` offset, the output matches a track registered upfront.
#[test]
fn video_track_added_after_start_uses_group_offset() {
    let (mut queue, mut input) =
        create_queue_with_input(QueueTrackGroupOffset::FromStart(Duration::from_millis(60)));
    sleep(OFFSET);
    queue.start();

    sleep(Duration::from_millis(1));
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &VideoBatch {
            pts: ms(0),
            required: false,
            frames: frames([]),
        },
    );
    assert!(queue.next_video_batch().is_none());

    input.add_video_track().unwrap();
    input.send_frame(ms(0));
    input.send_frame(ms(15));
    input.send_frame(ms(30));
    input.send_frame(ms(45));
    input.send_frame(ms(60));
    input.send_frame(ms(75));

    sleep(Duration::from_millis(20));
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20), true);
    assert!(queue.next_video_batch().is_none());

    sleep(Duration::from_millis(20));
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40), true);
    assert!(queue.next_video_batch().is_none());

    sleep(Duration::from_millis(20));
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(60), frame(0, ms(60))),
    );
    assert!(queue.next_video_batch().is_none());

    sleep(Duration::from_millis(20));
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(80), frame(1, ms(75))),
    );
    assert!(queue.next_video_batch().is_none());
}

/// Same as `audio::required_input::offset_from_start_delivered_slightly_early`, but the track
/// is added to an independent group.
#[test]
fn audio_track_uses_group_offset() {
    let (mut queue, mut input) =
        create_queue_with_input(QueueTrackGroupOffset::FromStart(Duration::from_millis(60)));
    input.add_audio_track().unwrap();
    sleep(OFFSET);
    queue.start();

    input.send_samples(ms(0), INPUT_BATCH_DURATION);
    input.send_samples(ms(15), INPUT_BATCH_DURATION);
    input.send_samples(ms(30), INPUT_BATCH_DURATION);
    input.send_samples(ms(45), INPUT_BATCH_DURATION);
    input.send_samples(ms(60), INPUT_BATCH_DURATION);
    input.send_samples(ms(75), INPUT_BATCH_DURATION);
    input.send_samples(ms(90), INPUT_BATCH_DURATION);
    input.send_samples(ms(105), INPUT_BATCH_DURATION);

    sleep(Duration::from_millis(1));
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
    assert!(queue.next_audio_batch().is_none());

    sleep(Duration::from_millis(20));
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
    assert!(queue.next_audio_batch().is_none());

    sleep(Duration::from_millis(20));
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(40),
            vec![
                (0, ms(60), ms(75)),
                (1, ms(75), ms(90)),
                (2, ms(90), ms(105)),
                (3, ms(105), ms(120)),
                (4, ms(120), ms(135)),
                (5, ms(135), ms(150)),
            ],
        ),
    );
    assert!(queue.next_audio_batch().is_none());

    sleep(Duration::from_millis(20));
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(ms(60), vec![(6, ms(150), ms(165))]),
    );
    assert!(queue.next_audio_batch().is_none());
}

/// A second video track added to the same group waits until the first one delivered EOS, then
/// continues the group's timeline.
#[test]
fn second_video_track_starts_after_first_eos() {
    let (mut queue, mut input) =
        create_queue_with_input(QueueTrackGroupOffset::FromStart(Duration::from_millis(60)));
    input.add_video_track().unwrap();
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));

    // closes the first track
    input.add_video_track().unwrap();
    input.send_frame(ms(60));
    input.send_frame(ms(80));
    input.end_video();

    sleep(OFFSET);
    queue.start();
    sleep(Duration::from_millis(150));

    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40), true);
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(60), frame(0, ms(60))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(80), frame(1, ms(80))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(100), frame_eos(2, ms(100))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(120), frame(3, ms(120))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(140), frame_eos(4, ms(140))),
    );
}

/// A track added to a group that is still pending starts when the queue switches to that group.
#[test]
fn track_added_to_pending_group_starts_after_switch() {
    let (mut queue, mut input) =
        create_queue_with_input(QueueTrackGroupOffset::FromStart(Duration::from_millis(60)));
    input.add_video_track().unwrap();
    input.send_frame(ms(0));
    input.send_frame(ms(20));

    // next group, the track is added while the first group is still running; this closes the
    // first track
    input.new_independent_track_group(QueueTrackGroupOffset::FromStart(Duration::from_millis(140)));
    input.add_video_track().unwrap();
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.end_video();

    sleep(OFFSET);
    queue.start();
    sleep(Duration::from_millis(170));

    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(0), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(20), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(40), true);
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(60), frame(0, ms(60))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(80), frame_eos(1, ms(80))),
    );

    // switched to the next group, its zero is at 140ms
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(100), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(120), true);
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(140), frame(2, ms(140))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(160), frame_eos(3, ms(160))),
    );
}

/// A group without tracks is replaced as soon as another group is pending; tracks can't be
/// added to it afterwards.
#[test]
fn add_track_to_ended_group_fails() {
    let (_queue, mut input) = create_queue_with_input(QueueTrackGroupOffset::None);
    let first_group = input.track_group.take().unwrap();
    input.new_independent_track_group(QueueTrackGroupOffset::None);

    // the queue switches groups before start too
    sleep(Duration::from_millis(10));

    assert!(first_group.add_video_track().is_err());
    assert!(first_group.add_audio_track().is_err());
    assert!(input.add_video_track().is_ok());
    assert!(input.add_audio_track().is_ok());
}

#[test]
fn add_track_after_input_removed_fails() {
    let (queue, mut input) = create_queue_with_input(QueueTrackGroupOffset::None);
    let track_group = input.track_group.take().unwrap();

    queue.queue.remove_input(&input.input_id);
    drop(input);

    assert!(track_group.add_video_track().is_err());
    assert!(track_group.add_audio_track().is_err());
}
