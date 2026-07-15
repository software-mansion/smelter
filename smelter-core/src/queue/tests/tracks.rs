//! Tests for back-to-back tracks on a single input ([`TestInput::new_track`]).
//!
//! Every track ends with an EOS: the event and the `eos` flag on the input's
//! entry in the output batch. The track switch happens only after the EOS was
//! emitted, and when the next track is already queued the switch happens
//! within the same batch that carries the EOS flag — its first frame/batches
//! are delivered alongside the flag, so a gapless track change never produces
//! a video batch without a frame.

use std::{thread::sleep, time::Duration};

use crate::queue::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use super::harness::{
    AudioBatch, BATCH_DURATION, InputFrame, InputSamples, OFFSET, TestInput, TestQueue,
    TestQueueOptions, VideoBatch, assert_audio_batch_eq, assert_audio_batch_eq_with_tolerance,
    assert_empty_video_batch, assert_video_batch_eq, assert_video_batch_eq_with_tolerance, frames,
    ms, samples,
};

fn frame(id: u32, pts: Duration) -> InputFrame {
    InputFrame {
        frame: Some((id, pts)),
        eos: false,
    }
}

/// A batch with a single entry from the required "input_1".
fn video_batch(pts: Duration, entry: InputFrame) -> VideoBatch {
    VideoBatch {
        pts,
        required: true,
        frames: frames([("input_1", entry)]),
    }
}

/// A chunk with a single entry from the required "input_1".
fn audio_chunk(start_pts: Duration, entry: InputSamples) -> AudioBatch {
    AudioBatch {
        start_pts,
        end_pts: start_pts + BATCH_DURATION,
        required: true,
        samples: samples([("input_1", entry)]),
    }
}

/// Create a queue with a single required input ("input_1") with a
/// `Pts(OFFSET)` first track (input PTS maps ~1:1 to queue PTS), desync the
/// clocks and start the queue.
fn start_queue_with_input(video: bool, audio: bool) -> (TestQueue, TestInput) {
    let mut queue = TestQueue::new(TestQueueOptions::default());
    let input = queue.add_input(
        "input_1",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
        QueueTrackOptions {
            video,
            audio,
            offset: QueueTrackOffset::Pts(OFFSET),
        },
    );

    // desync regular clock from queue clock
    sleep(OFFSET);

    queue.start();
    (queue, input)
}

#[test]
fn video_track_switch_gapless() {
    let (queue, mut input) = start_queue_with_input(true, false);

    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.end_video();
    // queue the next track before the current one ends and buffer its first
    // frame upfront
    input.new_track(QueueTrackOptions {
        video: true,
        audio: false,
        offset: QueueTrackOffset::None,
    });
    input.send_frame(ms(0)); // frame 3

    sleep(ms(1));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(0), frame(0, ms(0))),
        ms(2),
    );
    assert!(queue.next_video_batch().is_none());
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);

    sleep(ms(20));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(20), frame(1, ms(20))),
        ms(2),
    );
    assert!(queue.next_video_batch().is_none());

    sleep(ms(20));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(40), frame(2, ms(40))),
        ms(2),
    );
    assert!(queue.next_video_batch().is_none());

    // the old track ends and the pending one starts within the same batch:
    // the EOS flag rides alongside the first frame of the new track, there is
    // no batch without a frame
    sleep(ms(20));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(
            ms(60),
            InputFrame {
                frame: Some((3, ms(60))),
                eos: true,
            },
        ),
        ms(2),
    );
    assert!(queue.next_video_batch().is_none());
    queue.expect_events(&[
        input.video_eos_event(),
        input.video_delivered_event(),
        input.video_playing_event(),
    ]);

    // the new track continues as a regular track (a required input needs a
    // newer frame buffered before frame 4 can be released)
    input.send_frame(ms(20)); // frame 4
    input.send_frame(ms(40)); // frame 5
    sleep(ms(20));
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(80), frame(4, ms(80))),
    );
    assert!(queue.next_video_batch().is_none());
}

#[test]
fn video_eos_and_track_switch_after_eos() {
    let (queue, mut input) = start_queue_with_input(true, false);

    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.end_video();

    sleep(ms(1));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(0), frame(0, ms(0))),
        ms(2),
    );
    assert!(queue.next_video_batch().is_none());

    sleep(ms(20));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(20), frame(1, ms(20))),
        ms(2),
    );
    assert!(queue.next_video_batch().is_none());

    // without a pending track the EOS occupies a batch on its own
    sleep(ms(20));
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &video_batch(
            ms(40),
            InputFrame {
                frame: None,
                eos: true,
            },
        ),
    );
    assert!(queue.next_video_batch().is_none());
    queue.expect_events(&[
        input.video_delivered_event(),
        input.video_playing_event(),
        input.video_eos_event(),
    ]);

    // after the EOS the input stops contributing
    sleep(ms(20));
    assert_empty_video_batch(&queue.next_video_batch().unwrap(), ms(60));
    assert!(queue.next_video_batch().is_none());

    // a track queued after the EOS starts on the next batch, without a second
    // EOS of the old track
    input.new_track(QueueTrackOptions {
        video: true,
        audio: false,
        offset: QueueTrackOffset::None,
    });
    input.send_frame(ms(0)); // frame 2
    input.send_frame(ms(20)); // frame 3

    sleep(ms(20));
    assert_video_batch_eq(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(80), frame(2, ms(80))),
    );
    assert!(queue.next_video_batch().is_none());
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);
}

#[test]
fn audio_track_switch_gapless() {
    let (queue, mut input) = start_queue_with_input(false, true);

    input.send_samples(ms(0), BATCH_DURATION);
    input.send_samples(ms(20), BATCH_DURATION);
    input.send_samples(ms(40), BATCH_DURATION);
    input.send_samples(ms(60), BATCH_DURATION);
    input.send_samples(ms(80), BATCH_DURATION);
    input.end_audio();
    // queue the next track before the current one ends and buffer its first
    // batch upfront
    input.new_track(QueueTrackOptions {
        video: false,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_samples(ms(0), BATCH_DURATION);

    // after EOS readiness is unconditional, [0, 20) pops the whole track
    sleep(ms(1));
    assert_audio_batch_eq_with_tolerance(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(0),
            InputSamples {
                batches: vec![
                    (ms(0), ms(20)),
                    (ms(20), ms(40)),
                    (ms(40), ms(60)),
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                ],
                eos: false,
            },
        ),
        ms(2),
    );
    assert!(queue.next_audio_batch().is_none());
    queue.expect_events(&[input.audio_delivered_event(), input.audio_playing_event()]);

    // the old track ends and the pending one starts within the same chunk:
    // the EOS flag rides alongside the first batch of the new track
    sleep(ms(20));
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(20),
            InputSamples {
                batches: vec![(ms(20), ms(40))],
                eos: true,
            },
        ),
    );
    assert!(queue.next_audio_batch().is_none());
    queue.expect_events(&[
        input.audio_eos_event(),
        input.audio_delivered_event(),
        input.audio_playing_event(),
    ]);

    // the new track continues as a regular track once it is buffered ahead
    input.send_samples(ms(20), BATCH_DURATION);
    input.send_samples(ms(40), BATCH_DURATION);
    input.send_samples(ms(60), BATCH_DURATION);
    input.send_samples(ms(80), BATCH_DURATION);
    input.send_samples(ms(100), BATCH_DURATION);

    sleep(ms(20));
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(40),
            InputSamples {
                batches: vec![
                    (ms(40), ms(60)),
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                ],
                eos: false,
            },
        ),
    );
    assert!(queue.next_audio_batch().is_none());
}

#[test]
fn audio_eos_and_track_switch_after_eos() {
    let (queue, mut input) = start_queue_with_input(false, true);

    input.send_samples(ms(0), BATCH_DURATION);
    input.send_samples(ms(20), BATCH_DURATION);
    input.end_audio();

    sleep(ms(1));
    assert_audio_batch_eq_with_tolerance(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(0),
            InputSamples {
                batches: vec![(ms(0), ms(20)), (ms(20), ms(40))],
                eos: false,
            },
        ),
        ms(2),
    );
    assert!(queue.next_audio_batch().is_none());

    // without a pending track the EOS flag is emitted on an empty chunk
    sleep(ms(20));
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(20),
            InputSamples {
                batches: vec![],
                eos: true,
            },
        ),
    );
    assert!(queue.next_audio_batch().is_none());
    queue.expect_events(&[
        input.audio_delivered_event(),
        input.audio_playing_event(),
        input.audio_eos_event(),
    ]);

    // after the EOS the entry stays with an empty batch list
    sleep(ms(20));
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(40),
            InputSamples {
                batches: vec![],
                eos: false,
            },
        ),
    );
    assert!(queue.next_audio_batch().is_none());

    // a track queued after the EOS starts on the next chunk, without a second
    // EOS of the old track
    input.new_track(QueueTrackOptions {
        video: false,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_samples(ms(0), BATCH_DURATION);
    input.send_samples(ms(20), BATCH_DURATION);
    input.send_samples(ms(40), BATCH_DURATION);
    input.send_samples(ms(60), BATCH_DURATION);
    input.send_samples(ms(80), BATCH_DURATION);

    sleep(ms(20));
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(60),
            InputSamples {
                batches: vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                    (ms(140), ms(160)),
                ],
                eos: false,
            },
        ),
    );
    assert!(queue.next_audio_batch().is_none());
    queue.expect_events(&[input.audio_delivered_event(), input.audio_playing_event()]);
}

#[test]
fn av_track_switch_gapless() {
    let (queue, mut input) = start_queue_with_input(true, true);

    input.send_frame(ms(0));
    input.send_frame(ms(20));
    input.send_frame(ms(40));
    input.end_video();
    input.send_samples(ms(0), BATCH_DURATION);
    input.send_samples(ms(20), BATCH_DURATION);
    input.send_samples(ms(40), BATCH_DURATION);
    input.end_audio();
    input.new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    });
    input.send_frame(ms(0)); // frame 3
    input.send_samples(ms(0), BATCH_DURATION);

    sleep(ms(1));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(0), frame(0, ms(0))),
        ms(2),
    );
    assert_audio_batch_eq_with_tolerance(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(0),
            InputSamples {
                batches: vec![(ms(0), ms(20)), (ms(20), ms(40)), (ms(40), ms(60))],
                eos: false,
            },
        ),
        ms(2),
    );
    queue.expect_events_unordered(&[
        input.video_delivered_event(),
        input.audio_delivered_event(),
        input.video_playing_event(),
        input.audio_playing_event(),
    ]);

    // the audio track drains ~100ms ahead (stretch buffer), so its EOS is
    // emitted while the video track is still playing; the track switch waits
    // for the video side
    sleep(ms(20));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(20), frame(1, ms(20))),
        ms(2),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(20),
            InputSamples {
                batches: vec![],
                eos: true,
            },
        ),
    );
    queue.expect_events(&[input.audio_eos_event()]);

    sleep(ms(20));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(ms(40), frame(2, ms(40))),
        ms(2),
    );
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(40),
            InputSamples {
                batches: vec![],
                eos: false,
            },
        ),
    );

    // the video track ends: both tracks switch within this batch and the
    // first frame of the new track rides alongside the video EOS flag
    sleep(ms(20));
    assert_video_batch_eq_with_tolerance(
        &queue.next_video_batch().unwrap(),
        &video_batch(
            ms(60),
            InputFrame {
                frame: Some((3, ms(60))),
                eos: true,
            },
        ),
        ms(2),
    );
    assert!(queue.next_video_batch().is_none());
    // the new audio track needs to be buffered ahead before its first chunk
    assert!(queue.next_audio_batch().is_none());
    queue.expect_events(&[
        input.video_eos_event(),
        input.video_delivered_event(),
        input.video_playing_event(),
        input.audio_delivered_event(),
    ]);

    input.send_samples(ms(20), BATCH_DURATION);
    input.send_samples(ms(40), BATCH_DURATION);
    input.send_samples(ms(60), BATCH_DURATION);
    input.send_samples(ms(80), BATCH_DURATION);

    // both tracks of the new track share the offset resolved at the switch
    // point, so the new audio is aligned with the new video
    sleep(ms(1));
    assert_audio_batch_eq(
        &queue.next_audio_batch().unwrap(),
        &audio_chunk(
            ms(60),
            InputSamples {
                batches: vec![
                    (ms(60), ms(80)),
                    (ms(80), ms(100)),
                    (ms(100), ms(120)),
                    (ms(120), ms(140)),
                    (ms(140), ms(160)),
                ],
                eos: false,
            },
        ),
    );
    assert!(queue.next_audio_batch().is_none());
    queue.expect_events(&[input.audio_playing_event()]);
}
