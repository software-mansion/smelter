//! Track switching on a single input, as `Mp4Input` does when it loops: both
//! tracks are registered with [`QueueTrackOffset::None`], the next one is queued
//! while the previous is still draining, and the queue swaps once both media of
//! the old track delivered EOS.
//!
//! The two media of a track share a single `TrackOffset`, so whichever resolves
//! it first decides where that track's zero lands. The audio queue is processed
//! before the video queue on each tick and also carries the swap, so audio wins
//! that race and anchors the offset to the *audio* slot (the next 20ms chunk)
//! instead of the *video* slot (the next output batch).
//!
//! With the harness defaults both slots sit on the same 20ms grid, and within a
//! tick the queue pushes video before audio when they are equal. That makes the
//! gap deterministic: when the video EOS is the later of the two, the swap lands
//! on the iteration right after a video batch was pushed, so the audio slot is
//! one batch behind the video slot and the video looks up input PTS 20ms
//! instead of 0.
//!
//! Every test shares the same track 1: nothing for the first 50ms (three empty
//! batches), then 45ms of audio followed by video frames at 0/20/40/60ms. The
//! audio is sent first and given a tick to land, so it is the medium that
//! resolves the offset, pinning it to the 60ms slot. A chunk pops everything up
//! to chunk end + 80ms, so 45ms of audio drains on its first chunk and the video
//! EOS at 120ms is the later of the two (the last test inverts this).
//!
//! That puts the swap at the 140ms video slot with the offset at 120ms. Frame
//! ids keep incrementing across tracks, so the next track's first frame is
//! always id 4.

use std::{thread::sleep, time::Duration};

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{
    INPUT_BATCH_DURATION, InputFrame, TestInput, TestQueue, TestQueueOptions, VideoBatch,
    assert_empty_video_batch, assert_video_batch_eq, frames, ms,
};

fn frame(id: u32, pts: Duration) -> InputFrame {
    InputFrame::frame(id, pts)
}

fn frame_eos(id: u32, pts: Duration) -> InputFrame {
    InputFrame::frame_eos(id, pts)
}

/// A batch with a single frame from the required "input_1".
fn batch(pts: Duration, frame: InputFrame) -> VideoBatch {
    VideoBatch {
        pts,
        required: true,
        frames: frames([("input_1", frame)]),
    }
}

/// Create a queue with a single required input carrying both video and audio,
/// with its first track registered the way `Mp4Input` registers one without an
/// explicit offset. The queue is not started yet.
fn create_queue_with_av_input() -> (TestQueue, TestInput) {
    let queue = TestQueue::new(TestQueueOptions::default());
    let input = queue.add_input(
        "input_1",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
        QueueTrackOptions {
            video: true,
            audio: true,
            offset: QueueTrackOffset::None,
        },
    );
    (queue, input)
}

/// Both media of the next track start at 0 and its frames are 20ms apart, the
/// output batch duration.
///
/// The offset is locked to the audio slot at 120ms while the video slot is
/// already at 140ms, so the video looks up input PTS 20ms and `prepare_for_pts`
/// promotes past the track's first frame — the second one sits exactly at 20ms.
///
/// Expected at 140ms is `frame(4, ms(120))`; frame 4 is never rendered.
#[test]
fn next_track_equal_fps_drops_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    // nothing until 50ms, then audio first so it is the medium that resolves
    // the `None` offset, pinning it to the 60ms slot
    sleep(ms(50));
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 3);
    sleep(ms(2));
    for index in 0..4 {
        input.send_frame(ms(20) * index);
    }

    // next track, queued while track 1 is still draining. Sending both media up
    // front means the queue observes them on the same tick.
    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 10);
    input.stream_video_then_eos((0..12).map(|index| ms(20) * index).collect());

    sleep(ms(120));

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
        &batch(ms(100), frame(2, ms(100))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(120), frame_eos(3, ms(120))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(140), frame(5, ms(140))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(160), frame(6, ms(160))),
    );
}

/// Next track below the output framerate (frames 25ms apart).
///
/// Same 20ms lookup, but the second frame is at 25ms so `prepare_for_pts` has
/// nothing to promote to: the track's first frame survives, stamped 20ms before
/// the batch carrying it.
#[test]
fn next_track_lower_fps_keeps_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 3);
    sleep(ms(2));
    for index in 0..4 {
        input.send_frame(ms(20) * index);
    }

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 10);
    input.stream_video_then_eos((0..12).map(|index| ms(25) * index).collect());

    sleep(ms(120));

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
        &batch(ms(100), frame(2, ms(100))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(120), frame_eos(3, ms(120))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(140), frame(4, ms(120))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(160), frame(5, ms(145))),
    );
}

/// Next track above the output framerate (frames 10ms apart). The 20ms lookup
/// now covers two input frames, so the track starts 20ms into its media.
#[test]
fn next_track_higher_fps_drops_first_two_frames() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 3);
    sleep(ms(2));
    for index in 0..4 {
        input.send_frame(ms(20) * index);
    }

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 10);
    input.stream_video_then_eos((0..24).map(|index| ms(10) * index).collect());

    sleep(ms(120));

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
        &batch(ms(100), frame(2, ms(100))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(120), frame_eos(3, ms(120))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(140), frame(6, ms(140))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(160), frame(8, ms(160))),
    );
}

/// Next track's audio starts 20ms into its media while its video starts at 0.
///
/// The offset stores the queue PTS that observed the first packet, never the
/// packet's own PTS, so shifting the audio changes nothing for the video: the
/// result is identical to [`next_track_equal_fps_drops_first_frame`]. The audio
/// keeps its 20ms head start, delivered 20ms after the offset.
#[test]
fn next_track_audio_starting_later_drops_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 3);
    sleep(ms(2));
    for index in 0..4 {
        input.send_frame(ms(20) * index);
    }

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_sample_batches(ms(20), INPUT_BATCH_DURATION, 10);
    input.stream_video_then_eos((0..12).map(|index| ms(20) * index).collect());

    sleep(ms(120));

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
        &batch(ms(100), frame(2, ms(100))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(120), frame_eos(3, ms(120))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(140), frame(5, ms(140))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(160), frame(6, ms(160))),
    );
}

/// The other way round: the next track's video starts 20ms into its media while
/// its audio starts at 0.
///
/// The 20ms the video is short by is exactly the 20ms the lookup is ahead by, so
/// the two cancel and the track's first frame lands on the first batch. Nothing
/// about the offset changed — the track just happens to be missing the frames
/// that would otherwise be discarded.
#[test]
fn next_track_video_starting_later_keeps_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 3);
    sleep(ms(2));
    for index in 0..4 {
        input.send_frame(ms(20) * index);
    }

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 10);
    input.stream_video_then_eos((0..12).map(|index| ms(20) + ms(20) * index).collect());

    sleep(ms(120));

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
        &batch(ms(100), frame(2, ms(100))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(120), frame_eos(3, ms(120))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(140), frame(4, ms(140))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(160), frame(5, ms(160))),
    );
}

/// Next track's video starts 40ms into its media (what a leading empty edit in
/// the `elst` box produces), audio at 0.
///
/// Now the head start outruns the 20ms lookup: the first batch of the new track
/// carries no frame at all, and the track's first frame lands one batch later.
#[test]
fn next_track_video_starting_much_later_renders_empty_batch() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 3);
    sleep(ms(2));
    for index in 0..4 {
        input.send_frame(ms(20) * index);
    }

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 10);
    input.stream_video_then_eos((0..12).map(|index| ms(40) + ms(20) * index).collect());

    sleep(ms(120));

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
        &batch(ms(100), frame(2, ms(100))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(120), frame_eos(3, ms(120))),
    );
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(140), true);
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(160), frame(4, ms(160))),
    );
}

/// Track 1's audio outlives its video, so the *audio* EOS is the later one and
/// the swap lands right after an audio chunk was pushed. There both slots are
/// equal, the offset matches the video slot, the lookup is 0, and the next
/// track's first frame survives — the same scenario as
/// [`next_track_equal_fps_drops_first_frame`] except for which medium ended last.
///
/// It also shows the other seam problem: the swap waits for both media, so the
/// input delivers nothing between its video EOS at 120ms and the audio draining
/// at 280ms.
#[test]
fn next_track_after_audio_ends_last_keeps_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    // 300ms of audio: the buffer only empties on the chunk at 260ms
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 20);
    sleep(ms(2));
    for index in 0..4 {
        input.send_frame(ms(20) * index);
    }

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_sample_batches(ms(0), INPUT_BATCH_DURATION, 10);
    input.stream_video_then_eos((0..12).map(|index| ms(20) * index).collect());

    sleep(ms(260));

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
        &batch(ms(100), frame(2, ms(100))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(120), frame_eos(3, ms(120))),
    );

    // video ended, audio has not: nothing is delivered for 140ms
    for pts in [140, 160, 180, 200, 220, 240, 260] {
        assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(pts), true);
    }

    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(280), frame(4, ms(280))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(300), frame(5, ms(300))),
    );
}
