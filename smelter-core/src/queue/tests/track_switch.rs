//! Track switching on a single input, as `Mp4Input` does when it loops: both
//! tracks are registered with [`QueueTrackOffset::None`], the next one is queued
//! while the previous is still draining, and the queue swaps once both media of
//! the old track delivered EOS.
//!
//! The two media of a track share a single `TrackOffset`, so whichever resolves
//! it first decides where that track's zero lands. The queue processes the video
//! queue before the audio queue on each tick, so video both starts the pending
//! track and anchors the offset to the *video* slot — the batch it is about to
//! emit — which makes the first lookup on the new track exactly 0.
//!
//! That ordering matters because the two paths handle a non-zero lookup
//! differently: `prepare_for_pts` discards frames older than it, while
//! `pop_before_pts` only re-stamps sample batches. Anchoring on the audio slot
//! instead would put the offset one batch (20ms) earlier, and the tests below
//! would lose the new track's first frame.
//!
//! Every test shares the same track 1: nothing for the first 50ms (three empty
//! batches and chunks), then 45ms of audio and video frames at 0/20/40/60ms,
//! with the offset resolving to the 60ms slot. A chunk carries every batch up to
//! chunk end + 80ms (`MIXER_STRETCH_BUFFER`), so those 45ms of audio are all
//! delivered in the first chunk that sees them and the video EOS at 120ms is the
//! later of the two (the last test inverts this).
//!
//! That puts the swap at the 140ms video slot. Ids keep incrementing across
//! tracks, so the next track always starts with frame id 4 and sample batch
//! id 3.

use std::{thread::sleep, time::Duration};

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{
    AudioBatch, BATCH_DURATION, INPUT_BATCH_DURATION, InputFrame, InputSamples, TestInput,
    TestQueue, TestQueueOptions, VideoBatch, assert_audio_batch_eq, assert_empty_audio_batch,
    assert_empty_video_batch, assert_video_batch_eq, frames, ms, samples,
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

/// A chunk with sample batches from the required "input_1".
fn chunk(start_pts: Duration, batches: Vec<(u32, Duration, Duration)>) -> AudioBatch {
    AudioBatch {
        start_pts,
        end_pts: start_pts + BATCH_DURATION,
        required: true,
        samples: samples([("input_1", InputSamples::batches(batches))]),
    }
}

/// Like [`chunk`], but the track ended and EOS is delivered with these batches.
fn chunk_eos(start_pts: Duration, batches: Vec<(u32, Duration, Duration)>) -> AudioBatch {
    AudioBatch {
        start_pts,
        end_pts: start_pts + BATCH_DURATION,
        required: true,
        samples: samples([("input_1", InputSamples::batches_eos(batches))]),
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
/// output batch duration. The offset lands on the 140ms video slot, the lookup
/// is 0, and the track plays from its first frame. Its audio starts at 140ms
/// too, delivered ahead of time in the chunk that precedes it.
#[test]
fn next_track_equal_fps_keeps_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    // nothing until 50ms, so track 1's `None` offset resolves to the 60ms slot
    sleep(ms(50));
    input.send_samples(ms(0), INPUT_BATCH_DURATION);
    input.send_samples(ms(15), INPUT_BATCH_DURATION);
    input.send_samples(ms(30), INPUT_BATCH_DURATION);
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.send_frame(ms(60));

    // next track, queued while track 1 is still draining. Sending both media up
    // front means the queue observes them on the same tick.
    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    for index in 0..10 {
        input.send_samples(INPUT_BATCH_DURATION * index, INPUT_BATCH_DURATION);
    }
    for index in 0..12 {
        input.send_frame(ms(20) * index);
    }

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

    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk_eos(
            ms(60),
            vec![
                (0, ms(60), ms(75)),
                (1, ms(75), ms(90)),
                (2, ms(90), ms(105)),
            ],
        ),
    );
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(120),
            vec![
                (3, ms(140), ms(155)),
                (4, ms(155), ms(170)),
                (5, ms(170), ms(185)),
                (6, ms(185), ms(200)),
                (7, ms(200), ms(215)),
                (8, ms(215), ms(230)),
            ],
        ),
    );
}

/// Next track below the output framerate (frames 25ms apart). The first frame
/// still lands on the first batch, and is repeated on the next one because the
/// track has nothing newer to offer yet. Audio is unaffected by the framerate.
#[test]
fn next_track_lower_fps_keeps_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_samples(ms(0), INPUT_BATCH_DURATION);
    input.send_samples(ms(15), INPUT_BATCH_DURATION);
    input.send_samples(ms(30), INPUT_BATCH_DURATION);
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.send_frame(ms(60));

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    for index in 0..10 {
        input.send_samples(INPUT_BATCH_DURATION * index, INPUT_BATCH_DURATION);
    }
    for index in 0..12 {
        input.send_frame(ms(25) * index);
    }

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
        &batch(ms(160), frame(4, ms(140))),
    );

    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk_eos(
            ms(60),
            vec![
                (0, ms(60), ms(75)),
                (1, ms(75), ms(90)),
                (2, ms(90), ms(105)),
            ],
        ),
    );
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(120),
            vec![
                (3, ms(140), ms(155)),
                (4, ms(155), ms(170)),
                (5, ms(170), ms(185)),
                (6, ms(185), ms(200)),
                (7, ms(200), ms(215)),
                (8, ms(215), ms(230)),
            ],
        ),
    );
}

/// Next track above the output framerate (frames 10ms apart). The first frame
/// lands on the first batch; the decimation only starts on the next one, where
/// the 20ms lookup skips over frame 5.
#[test]
fn next_track_higher_fps_keeps_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_samples(ms(0), INPUT_BATCH_DURATION);
    input.send_samples(ms(15), INPUT_BATCH_DURATION);
    input.send_samples(ms(30), INPUT_BATCH_DURATION);
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.send_frame(ms(60));

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    for index in 0..10 {
        input.send_samples(INPUT_BATCH_DURATION * index, INPUT_BATCH_DURATION);
    }
    for index in 0..24 {
        input.send_frame(ms(10) * index);
    }

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
        &batch(ms(160), frame(6, ms(160))),
    );

    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk_eos(
            ms(60),
            vec![
                (0, ms(60), ms(75)),
                (1, ms(75), ms(90)),
                (2, ms(90), ms(105)),
            ],
        ),
    );
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(120),
            vec![
                (3, ms(140), ms(155)),
                (4, ms(155), ms(170)),
                (5, ms(170), ms(185)),
                (6, ms(185), ms(200)),
                (7, ms(200), ms(215)),
                (8, ms(215), ms(230)),
            ],
        ),
    );
}

/// Next track's audio starts 20ms into its media while its video starts at 0.
///
/// The offset stores the queue PTS that observed the first packet, never the
/// packet's own PTS, so shifting the audio changes nothing for the video: the
/// video output is identical to [`next_track_equal_fps_keeps_first_frame`]. The
/// audio keeps its 20ms head start, landing at 160ms instead of 140ms.
#[test]
fn next_track_audio_starting_later_keeps_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_samples(ms(0), INPUT_BATCH_DURATION);
    input.send_samples(ms(15), INPUT_BATCH_DURATION);
    input.send_samples(ms(30), INPUT_BATCH_DURATION);
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.send_frame(ms(60));

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    for index in 0..10 {
        input.send_samples(ms(20) + INPUT_BATCH_DURATION * index, INPUT_BATCH_DURATION);
    }
    for index in 0..12 {
        input.send_frame(ms(20) * index);
    }

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

    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk_eos(
            ms(60),
            vec![
                (0, ms(60), ms(75)),
                (1, ms(75), ms(90)),
                (2, ms(90), ms(105)),
            ],
        ),
    );
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(120),
            vec![
                (3, ms(160), ms(175)),
                (4, ms(175), ms(190)),
                (5, ms(190), ms(205)),
                (6, ms(205), ms(220)),
            ],
        ),
    );
}

/// The other way round: the next track's video starts 20ms into its media while
/// its audio starts at 0.
///
/// The offset is still the video slot, so the track's head start is preserved
/// rather than absorbed: the first batch of the new track carries no frame and
/// the first frame lands one batch later, 20ms after the offset. The audio still
/// starts on the offset, so the 20ms gap between the two media is kept.
#[test]
fn next_track_video_starting_later_renders_empty_batch() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_samples(ms(0), INPUT_BATCH_DURATION);
    input.send_samples(ms(15), INPUT_BATCH_DURATION);
    input.send_samples(ms(30), INPUT_BATCH_DURATION);
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.send_frame(ms(60));

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    for index in 0..10 {
        input.send_samples(INPUT_BATCH_DURATION * index, INPUT_BATCH_DURATION);
    }
    for index in 0..12 {
        input.send_frame(ms(20) + ms(20) * index);
    }

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

    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk_eos(
            ms(60),
            vec![
                (0, ms(60), ms(75)),
                (1, ms(75), ms(90)),
                (2, ms(90), ms(105)),
            ],
        ),
    );
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(120),
            vec![
                (3, ms(140), ms(155)),
                (4, ms(155), ms(170)),
                (5, ms(170), ms(185)),
                (6, ms(185), ms(200)),
                (7, ms(200), ms(215)),
                (8, ms(215), ms(230)),
            ],
        ),
    );
}

/// Next track's video starts 40ms into its media (what a leading empty edit in
/// the `elst` box produces), audio at 0. Same as the 20ms case, two batches
/// wide.
#[test]
fn next_track_video_starting_much_later_renders_empty_batches() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    input.send_samples(ms(0), INPUT_BATCH_DURATION);
    input.send_samples(ms(15), INPUT_BATCH_DURATION);
    input.send_samples(ms(30), INPUT_BATCH_DURATION);
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.send_frame(ms(60));

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    for index in 0..10 {
        input.send_samples(INPUT_BATCH_DURATION * index, INPUT_BATCH_DURATION);
    }
    for index in 0..12 {
        input.send_frame(ms(40) + ms(20) * index);
    }

    sleep(ms(140));

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
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(160), true);
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(180), frame(4, ms(180))),
    );

    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk_eos(
            ms(60),
            vec![
                (0, ms(60), ms(75)),
                (1, ms(75), ms(90)),
                (2, ms(90), ms(105)),
            ],
        ),
    );
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(80), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(100), true);
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(120),
            vec![
                (3, ms(140), ms(155)),
                (4, ms(155), ms(170)),
                (5, ms(170), ms(185)),
                (6, ms(185), ms(200)),
                (7, ms(200), ms(215)),
                (8, ms(215), ms(230)),
            ],
        ),
    );
}

/// Track 1's audio outlives its video, so the *audio* EOS is the later one and
/// the swap lands one iteration later, right after an audio chunk was pushed.
/// The offset is still the video slot and the outcome matches
/// [`next_track_equal_fps_keeps_first_frame`], shifted to the 280ms slot.
///
/// It also shows the seam problem the offset ordering does not fix: the swap
/// waits for both media, so the video delivers nothing between its EOS at 120ms
/// and the audio draining at 280ms.
#[test]
fn next_track_after_audio_ends_last_keeps_first_frame() {
    let (mut queue, mut input) = create_queue_with_av_input();
    queue.start();

    sleep(ms(50));
    // 300ms of audio: the buffer only empties on the chunk at 260ms
    for index in 0..20 {
        input.send_samples(INPUT_BATCH_DURATION * index, INPUT_BATCH_DURATION);
    }
    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.send_frame(ms(60));

    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    for index in 0..10 {
        input.send_samples(INPUT_BATCH_DURATION * index, INPUT_BATCH_DURATION);
    }
    for index in 0..12 {
        input.send_frame(ms(20) * index);
    }

    sleep(ms(290));

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
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(140), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(160), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(180), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(200), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(220), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(240), true);
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(260), true);

    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(280), frame(4, ms(280))),
    );
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &batch(ms(300), frame(5, ms(300))),
    );

    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(0), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(20), true);
    assert_empty_audio_batch(&queue.next_audio_batch().unwrap(), ms(40), true);

    // track 1's audio keeps draining while the video is already over
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(60),
            vec![
                (0, ms(60), ms(75)),
                (1, ms(75), ms(90)),
                (2, ms(90), ms(105)),
                (3, ms(105), ms(120)),
                (4, ms(120), ms(135)),
                (5, ms(135), ms(150)),
                (6, ms(150), ms(165)),
            ],
        ),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(ms(80), vec![(7, ms(165), ms(180))]),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(ms(100), vec![(8, ms(180), ms(195)), (9, ms(195), ms(210))]),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(ms(120), vec![(10, ms(210), ms(225))]),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(ms(140), vec![(11, ms(225), ms(240))]),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(160),
            vec![(12, ms(240), ms(255)), (13, ms(255), ms(270))],
        ),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(ms(180), vec![(14, ms(270), ms(285))]),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(ms(200), vec![(15, ms(285), ms(300))]),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(220),
            vec![(16, ms(300), ms(315)), (17, ms(315), ms(330))],
        ),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(ms(240), vec![(18, ms(330), ms(345))]),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk_eos(ms(260), vec![(19, ms(345), ms(360))]),
    );

    // swap happened right after that chunk, so the next track's audio is
    // anchored to the 280ms video slot
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &chunk(
            ms(280),
            vec![
                (20, ms(280), ms(295)),
                (21, ms(295), ms(310)),
                (22, ms(310), ms(325)),
                (23, ms(325), ms(340)),
                (24, ms(340), ms(355)),
                (25, ms(355), ms(370)),
                (26, ms(370), ms(385)),
            ],
        ),
    );
}
