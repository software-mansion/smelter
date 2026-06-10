mod harness;

use std::{
    thread,
    time::{Duration, Instant},
};

use crossbeam_channel::unbounded;

use super::{QueueInputOptions, QueueTrackOffset, QueueTrackOptions};

use self::harness::{
    AudioBatch, BATCH_DURATION, InputFrame, InputFrame::Eos, InputSamples, InputSamples::Batches,
    TestInput, TestQueue, TestQueueOptions, VideoBatch, assert_audio_batches_eq,
    assert_video_batches_eq, frames, ms, samples,
};

/// Tolerance for frame PTS values that depend on the real clock: how far the
/// queue start drifts from the reference point of the track offset (setup or
/// scheduling latency, typically well under a millisecond). Batch PTS, frame
/// ids and required flags are still compared exactly.
const PTS_TOLERANCE: Duration = Duration::from_millis(15);

const EXACT: Duration = Duration::ZERO;

fn frame(id: u32, pts: Duration) -> InputFrame {
    InputFrame::Frame { id, pts }
}

fn batch(pts: Duration, required: bool, frame: InputFrame) -> VideoBatch {
    VideoBatch {
        pts,
        required,
        frames: frames([("input_1", frame)]),
    }
}

fn audio_batch(start_pts: Duration, required: bool, content: InputSamples) -> AudioBatch {
    AudioBatch {
        start_pts,
        end_pts: start_pts + BATCH_DURATION,
        required,
        samples: samples([("input_1", content)]),
    }
}

fn batch2(pts: Duration, required: bool, frame_1: InputFrame, frame_2: InputFrame) -> VideoBatch {
    VideoBatch {
        pts,
        required,
        frames: frames([("input_1", frame_1), ("input_2", frame_2)]),
    }
}

/// Batch produced when no input delivers anything.
fn empty_batch(pts: Duration) -> VideoBatch {
    VideoBatch {
        pts,
        required: false,
        frames: frames([]),
    }
}

fn test_queue_with_track(
    required: bool,
    offset: QueueTrackOffset,
    video: bool,
    audio: bool,
) -> (TestQueue, TestInput) {
    let queue = TestQueue::new(TestQueueOptions::default());
    let input = queue.add_input(
        "input_1",
        QueueInputOptions {
            required,
            ..Default::default()
        },
        QueueTrackOptions {
            video,
            audio,
            offset,
        },
    );
    (queue, input)
}

fn test_queue_with_input(required: bool, offset: QueueTrackOffset) -> (TestQueue, TestInput) {
    test_queue_with_track(required, offset, true, false)
}

fn test_queue_with_audio_input(required: bool, offset: QueueTrackOffset) -> (TestQueue, TestInput) {
    test_queue_with_track(required, offset, false, true)
}

fn test_queue_with_av_input(required: bool, offset: QueueTrackOffset) -> (TestQueue, TestInput) {
    test_queue_with_track(required, offset, true, true)
}

//
// Each test in the required x offset x before/after-start matrix sends 5
// frames (PTS 0..80ms, one per output batch at 50 fps) and closes the stream.
//
// FromStart offset: track zero is placed at `start_pts + offset`, so input PTS
// map 1:1 to output PTS regardless of when frames were sent.
//

#[test]
fn offset_from_start_required_frames_after_start() {
    let (mut queue, mut input) =
        test_queue_with_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    // delivered/playing events are emitted when the first frame reaches the output
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(0)))],
        EXACT,
    );
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    // EOS batches are always marked as required
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_from_start_optional_frames_after_start() {
    let (mut queue, mut input) =
        test_queue_with_input(false, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), false, frame(0, ms(0)))],
        EXACT,
    );
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), false, frame(1, ms(20))),
            batch(ms(40), false, frame(2, ms(40))),
            batch(ms(60), false, frame(3, ms(60))),
            batch(ms(80), false, frame(4, ms(80))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_from_start_required_frames_before_start() {
    let (mut queue, mut input) =
        test_queue_with_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    // sends block on queue backpressure until the queue starts draining the input
    input.stream_video_then_eos((0..5).map(|index| BATCH_DURATION * index).collect());
    // block until the input is observed (at a pre-start cleanup tick) so
    // start always happens after delivery, regardless of scheduling jitter
    queue.wait_for_events(&[input.video_delivered_event()]);
    queue.start();

    // frames sent before start just wait in the input buffer; output is
    // identical to the after-start case
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(0)))],
        EXACT,
    );
    queue.expect_events(&[input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_from_start_optional_frames_before_start() {
    let (mut queue, mut input) =
        test_queue_with_input(false, QueueTrackOffset::FromStart(Duration::ZERO));

    input.stream_video_then_eos((0..5).map(|index| BATCH_DURATION * index).collect());
    // block until the input is observed (at a pre-start cleanup tick) so
    // start always happens after delivery, regardless of scheduling jitter
    queue.wait_for_events(&[input.video_delivered_event()]);
    queue.start();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), false, frame(0, ms(0)))],
        EXACT,
    );
    queue.expect_events(&[input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), false, frame(1, ms(20))),
            batch(ms(40), false, frame(2, ms(40))),
            batch(ms(60), false, frame(3, ms(60))),
            batch(ms(80), false, frame(4, ms(80))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

//
// None offset: the offset is initialized to the queue PTS of the batch that
// first observes the input. With a single input that already has frames
// buffered at start, that is the first batch after start, so the result is
// the same as `FromStart(0)`.
//

#[test]
fn offset_none_required_frames_after_start() {
    let (mut queue, mut input) = test_queue_with_input(true, QueueTrackOffset::None);

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(0)))],
        EXACT,
    );
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_none_optional_frames_after_start() {
    let (mut queue, mut input) = test_queue_with_input(false, QueueTrackOffset::None);

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), false, frame(0, ms(0)))],
        EXACT,
    );
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), false, frame(1, ms(20))),
            batch(ms(40), false, frame(2, ms(40))),
            batch(ms(60), false, frame(3, ms(60))),
            batch(ms(80), false, frame(4, ms(80))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_none_required_frames_before_start() {
    let (mut queue, mut input) = test_queue_with_input(true, QueueTrackOffset::None);

    input.stream_video_then_eos((0..5).map(|index| BATCH_DURATION * index).collect());
    // block until the input is observed (at a pre-start cleanup tick) so
    // start always happens after delivery, regardless of scheduling jitter
    queue.wait_for_events(&[input.video_delivered_event()]);
    // The offset was just initialized from the clock. Frames sit exactly on
    // batch boundaries, so whether a frame lands in the batch at its PTS or
    // the next one depends on a nanosecond-level race between the offset
    // initialization and `start` reading the clock; bias start a few ms
    // later to keep the mapping stable.
    thread::sleep(ms(5));
    queue.start();

    // The offset is initialized when the input is first observed, at the
    // pre-start cleanup tick. The queue starts right after that, so frames
    // stay aligned with the output batch PTS.
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(0)))],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
        ],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_none_optional_frames_before_start() {
    let (mut queue, mut input) = test_queue_with_input(false, QueueTrackOffset::None);

    input.stream_video_then_eos((0..5).map(|index| BATCH_DURATION * index).collect());
    // block until the input is observed (at a pre-start cleanup tick) so
    // start always happens after delivery, regardless of scheduling jitter
    queue.wait_for_events(&[input.video_delivered_event()]);
    // bias start past the offset initialization, see the required variant
    thread::sleep(ms(5));
    queue.start();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), false, frame(0, ms(0)))],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), false, frame(1, ms(20))),
            batch(ms(40), false, frame(2, ms(40))),
            batch(ms(60), false, frame(3, ms(60))),
            batch(ms(80), false, frame(4, ms(80))),
        ],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

//
// Pts offset: track zero is placed at `sync_point + offset`. The tests use
// `Pts(20ms)`.
//
// - After start: the queue starts right after `sync_point`, so the whole
//   stream lies ~20ms in the future. The first frame is repeated in batches 0
//   and 1, then every frame is delivered one batch later than its input PTS.
// - Before start: the queue starts right after the first pre-start cleanup
//   tick observes the input, ~20ms after `sync_point`. That cancels the 20ms
//   offset, so frames stay aligned with the output batch PTS.
//

#[test]
fn offset_pts_required_frames_after_start() {
    let (mut queue, mut input) = test_queue_with_input(true, QueueTrackOffset::Pts(ms(20)));

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(20)))],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(5),
        &[
            batch(ms(20), true, frame(0, ms(20))),
            batch(ms(40), true, frame(1, ms(40))),
            batch(ms(60), true, frame(2, ms(60))),
            batch(ms(80), true, frame(3, ms(80))),
            batch(ms(100), true, frame(4, ms(100))),
        ],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(120), true, Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_pts_optional_frames_after_start() {
    let (mut queue, mut input) = test_queue_with_input(false, QueueTrackOffset::Pts(ms(20)));

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), false, frame(0, ms(20)))],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(5),
        &[
            batch(ms(20), false, frame(0, ms(20))),
            batch(ms(40), false, frame(1, ms(40))),
            batch(ms(60), false, frame(2, ms(60))),
            batch(ms(80), false, frame(3, ms(80))),
            batch(ms(100), false, frame(4, ms(100))),
        ],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(120), true, Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_pts_required_frames_before_start() {
    let (mut queue, mut input) = test_queue_with_input(true, QueueTrackOffset::Pts(ms(20)));

    input.stream_video_then_eos((0..5).map(|index| BATCH_DURATION * index).collect());
    // block until the input is observed (at a pre-start cleanup tick) so
    // start always happens after delivery, regardless of scheduling jitter
    queue.wait_for_events(&[input.video_delivered_event()]);
    queue.start();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(0)))],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
        ],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn offset_pts_optional_frames_before_start() {
    let (mut queue, mut input) = test_queue_with_input(false, QueueTrackOffset::Pts(ms(20)));

    input.stream_video_then_eos((0..5).map(|index| BATCH_DURATION * index).collect());
    // block until the input is observed (at a pre-start cleanup tick) so
    // start always happens after delivery, regardless of scheduling jitter
    queue.wait_for_events(&[input.video_delivered_event()]);
    queue.start();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), false, frame(0, ms(0)))],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), false, frame(1, ms(20))),
            batch(ms(40), false, frame(2, ms(40))),
            batch(ms(60), false, frame(3, ms(60))),
            batch(ms(80), false, frame(4, ms(80))),
        ],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

//
// Audio-only tests. Audio chunks pop every input batch that starts within
// 80ms (mixer stretch buffer) past the chunk end, so the whole 100ms stream
// is delivered in the first chunk and EOS follows in the second.
//
// Optional audio inputs don't wait for data, so the content of a chunk is
// only deterministic when the samples are already buffered when the chunk is
// produced; the optional after-start test sends a single batch for that
// reason.
//

#[test]
fn audio_offset_from_start_required_frames_after_start() {
    let (mut queue, mut input) =
        test_queue_with_audio_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    input.send_sample_batches(ms(0), BATCH_DURATION, 5);
    input.end_audio();

    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(
            ms(0),
            true,
            Batches(vec![
                (ms(0), ms(20)),
                (ms(20), ms(40)),
                (ms(40), ms(60)),
                (ms(60), ms(80)),
                (ms(80), ms(100)),
            ]),
        )],
        EXACT,
    );
    queue.wait_for_events(&[input.audio_delivered_event(), input.audio_playing_event()]);

    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(ms(20), true, InputSamples::Eos)],
        EXACT,
    );
    queue.expect_events(&[input.audio_eos_event()]);
}

#[test]
fn audio_offset_from_start_optional_frames_after_start() {
    let (mut queue, mut input) =
        test_queue_with_audio_input(false, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    input.send_samples(ms(0), BATCH_DURATION);
    input.end_audio();

    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(ms(0), false, Batches(vec![(ms(0), ms(20))]))],
        EXACT,
    );
    queue.wait_for_events(&[input.audio_delivered_event(), input.audio_playing_event()]);

    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(ms(20), true, InputSamples::Eos)],
        EXACT,
    );
    queue.expect_events(&[input.audio_eos_event()]);
}

#[test]
fn audio_offset_from_start_required_frames_before_start() {
    let (mut queue, mut input) =
        test_queue_with_audio_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    input.stream_audio_then_eos(
        (0..5).map(|index| BATCH_DURATION * index).collect(),
        BATCH_DURATION,
    );
    queue.wait_for_events(&[input.audio_delivered_event()]);
    queue.start();

    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(
            ms(0),
            true,
            Batches(vec![
                (ms(0), ms(20)),
                (ms(20), ms(40)),
                (ms(40), ms(60)),
                (ms(60), ms(80)),
                (ms(80), ms(100)),
            ]),
        )],
        EXACT,
    );
    queue.wait_for_events(&[input.audio_playing_event()]);

    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(ms(20), true, InputSamples::Eos)],
        EXACT,
    );
    queue.expect_events(&[input.audio_eos_event()]);
}

#[test]
fn audio_offset_none_required_frames_before_start() {
    let (mut queue, mut input) = test_queue_with_audio_input(true, QueueTrackOffset::None);

    input.stream_audio_then_eos(
        (0..5).map(|index| BATCH_DURATION * index).collect(),
        BATCH_DURATION,
    );
    queue.wait_for_events(&[input.audio_delivered_event()]);
    queue.start();

    // Like in the video test: the offset is initialized at the pre-start
    // cleanup tick that observes the input, and the queue starts right after,
    // so sample batches stay aligned with the output chunk PTS.
    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(
            ms(0),
            true,
            Batches(vec![
                (ms(0), ms(20)),
                (ms(20), ms(40)),
                (ms(40), ms(60)),
                (ms(60), ms(80)),
                (ms(80), ms(100)),
            ]),
        )],
        PTS_TOLERANCE,
    );
    queue.wait_for_events(&[input.audio_playing_event()]);

    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(ms(20), true, InputSamples::Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.audio_eos_event()]);
}

//
// Audio + video tests: both tracks of one input share a single track offset,
// so video and audio stay in sync regardless of which track resolves the
// offset first.
//

#[test]
fn audio_video_offset_from_start_required_frames_after_start() {
    let (mut queue, mut input) =
        test_queue_with_av_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();
    input.send_sample_batches(ms(0), BATCH_DURATION, 5);
    input.end_audio();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(0)))],
        EXACT,
    );
    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(
            ms(0),
            true,
            Batches(vec![
                (ms(0), ms(20)),
                (ms(20), ms(40)),
                (ms(40), ms(60)),
                (ms(60), ms(80)),
                (ms(80), ms(100)),
            ]),
        )],
        EXACT,
    );
    // the whole audio stream fits in the first chunk, EOS follows right after
    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(ms(20), true, InputSamples::Eos)],
        EXACT,
    );
    // the relative order of audio and video events depends on which tick
    // observes which track first
    queue.expect_events_unordered(&[
        input.video_delivered_event(),
        input.video_playing_event(),
        input.audio_delivered_event(),
        input.audio_playing_event(),
        input.audio_eos_event(),
    ]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn audio_video_offset_none_required_frames_before_start() {
    let (mut queue, mut input) = test_queue_with_av_input(true, QueueTrackOffset::None);

    input.stream_video_then_eos((0..5).map(|index| BATCH_DURATION * index).collect());
    input.stream_audio_then_eos(
        (0..5).map(|index| BATCH_DURATION * index).collect(),
        BATCH_DURATION,
    );
    // the pre-start cleanup processes video first, then audio
    queue.wait_for_events(&[input.video_delivered_event(), input.audio_delivered_event()]);
    // bias start past the offset initialization, see offset_none_required_frames_before_start
    thread::sleep(ms(5));
    queue.start();

    // The shared track offset was initialized once at the pre-start cleanup
    // tick that observed the input: both video and audio stay aligned with
    // the output PTS and in sync with each other.
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(0)))],
        PTS_TOLERANCE,
    );
    // KNOWN ISSUE (surfaced by this test): the first audio batch is missing
    // below. The video queue initializes the shared offset first, so when the
    // audio side of the same cleanup tick reads the clock microseconds later,
    // the first audio batch (start_pts equal to the resolved offset) is
    // already considered old and gets dropped before start. Video keeps its
    // first frame, so the input starts with 20ms of missing audio. If
    // `drop_old_samples_before_start` ever drops relative to the offset
    // initialization instant instead of a fresh clock read, update this
    // expectation to include the (0ms, 20ms) batch.
    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(
            ms(0),
            true,
            Batches(vec![
                (ms(20), ms(40)),
                (ms(40), ms(60)),
                (ms(60), ms(80)),
                (ms(80), ms(100)),
            ]),
        )],
        PTS_TOLERANCE,
    );
    queue.wait_for_events(&[input.video_playing_event(), input.audio_playing_event()]);

    assert_audio_batches_eq(
        &[queue.next_audio_batch()],
        &[audio_batch(ms(20), true, InputSamples::Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.audio_eos_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
        ],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        PTS_TOLERANCE,
    );
    queue.expect_events(&[input.video_eos_event()]);
}

//
// Multi-track tests: an input can queue more tracks with `queue_new_track`;
// the queue switches to the next one when the current track ends, or
// immediately on `abort_old_track`.
//

#[test]
fn second_track_starts_after_first_track_ends() {
    let (mut queue, mut input) =
        test_queue_with_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 3);
    input.end_video();

    // queue the next track up front; sends block until the queue switches to it
    input.new_track(QueueTrackOptions {
        video: true,
        audio: false,
        offset: QueueTrackOffset::FromStart(ms(60)),
    });
    input.send_frames(ms(0), BATCH_DURATION, 3);
    input.end_video();

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, frame(0, ms(0)))],
        EXACT,
    );
    queue.wait_for_events(&[input.video_delivered_event(), input.video_playing_event()]);

    // The first track plays to its end and the second one takes over with no
    // EOS batch or event in between.
    assert_video_batches_eq(
        &queue.next_video_batches(5),
        &[
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
            batch(ms(100), true, frame(5, ms(100))),
        ],
        EXACT,
    );
    // the second track emits delivered/playing again
    queue.wait_for_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(120), true, Eos)],
        EXACT,
    );
    queue.wait_for_events(&[input.video_eos_event()]);
}

#[test]
fn abort_old_track_switches_immediately() {
    let (mut queue, mut input) =
        test_queue_with_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    // no `end_video`: the first track would keep playing until aborted
    input.send_frames(ms(0), BATCH_DURATION, 5);

    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[
            batch(ms(0), true, frame(0, ms(0))),
            batch(ms(20), true, frame(1, ms(20))),
        ],
        EXACT,
    );
    queue.wait_for_events(&[input.video_delivered_event(), input.video_playing_event()]);

    // Switch to a new track mid-stream: the remaining frames of the first
    // track are discarded without an EOS batch or event.
    input.new_track(QueueTrackOptions {
        video: true,
        audio: false,
        offset: QueueTrackOffset::None,
    });
    input.queue_input.abort_old_track();
    input.send_frames(ms(0), BATCH_DURATION, 3);
    input.end_video();

    // `None` offset resolves to the first batch processed after the switch
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(40), true, frame(5, ms(40)))],
        EXACT,
    );
    queue.wait_for_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[
            batch(ms(60), true, frame(6, ms(60))),
            batch(ms(80), true, frame(7, ms(80))),
        ],
        EXACT,
    );
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.wait_for_events(&[input.video_eos_event()]);
}

//
// Pause/resume
//

#[test]
fn pause_and_resume() {
    let (mut queue, mut input) =
        test_queue_with_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[
            batch(ms(0), true, frame(0, ms(0))),
            batch(ms(20), true, frame(1, ms(20))),
        ],
        EXACT,
    );
    queue.wait_for_events(&[input.video_delivered_event(), input.video_playing_event()]);

    // The queue's last processed PTS already points at the next batch (40ms)
    // right after batch 1 was pushed; sleep past the tick that processed it so
    // the pause point is stable.
    thread::sleep(ms(5));
    input.queue_input.pause();
    queue.expect_events(&[input.video_paused_event()]);

    // While paused, the frame at the pause point (the one that would play at
    // 40ms) is repeated with PTS advancing along the output.
    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(2, ms(60))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    // resume at ~65ms; the queue's last processed PTS points at 80ms
    thread::sleep(ms(5));
    input.queue_input.resume();

    // The pause duration (80ms - 40ms) was added to the track offset: the
    // remaining frames play 40ms later than their original PTS.
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(80), true, frame(2, ms(80)))],
        EXACT,
    );
    queue.wait_for_events(&[input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[
            batch(ms(100), true, frame(3, ms(100))),
            batch(ms(120), true, frame(4, ms(120))),
        ],
        EXACT,
    );
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(140), true, Eos)],
        EXACT,
    );
    queue.wait_for_events(&[input.video_eos_event()]);
}

//
// Multiple inputs
//

#[test]
fn required_input_stalls_queue_until_its_frames_arrive() {
    let queue = TestQueue::new(TestQueueOptions::default());
    let mut required_input = queue.add_input(
        "input_1",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
        QueueTrackOptions {
            video: true,
            audio: false,
            offset: QueueTrackOffset::FromStart(Duration::ZERO),
        },
    );
    let mut optional_input = queue.add_input(
        "input_2",
        QueueInputOptions::default(),
        QueueTrackOptions {
            video: true,
            audio: false,
            offset: QueueTrackOffset::FromStart(Duration::ZERO),
        },
    );
    let mut queue = queue;

    queue.start();
    optional_input.send_frames(ms(0), BATCH_DURATION, 5);
    optional_input.end_video();

    // no output at all while the required input has no data, even though the
    // optional input is ready and wall time is passing
    queue.expect_no_video_batch(ms(50));
    queue.expect_events(&[optional_input.video_delivered_event()]);

    required_input.send_frames(ms(0), BATCH_DURATION, 5);
    required_input.end_video();

    // batches burst out late, with frames of both inputs matched by PTS
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch2(ms(0), true, frame(0, ms(0)), frame(0, ms(0)))],
        EXACT,
    );
    queue.expect_events_unordered(&[
        required_input.video_delivered_event(),
        required_input.video_playing_event(),
        optional_input.video_playing_event(),
    ]);

    assert_video_batches_eq(
        &queue.next_video_batches(4),
        &[
            batch2(ms(20), true, frame(1, ms(20)), frame(1, ms(20))),
            batch2(ms(40), true, frame(2, ms(40)), frame(2, ms(40))),
            batch2(ms(60), true, frame(3, ms(60)), frame(3, ms(60))),
            batch2(ms(80), true, frame(4, ms(80)), frame(4, ms(80))),
        ],
        EXACT,
    );
    queue.expect_events(&[]);

    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch2(ms(100), true, Eos, Eos)],
        EXACT,
    );
    queue.expect_events_unordered(&[
        required_input.video_eos_event(),
        optional_input.video_eos_event(),
    ]);
}

//
// Ahead-of-time processing
//

#[test]
fn ahead_of_time_processing_produces_output_faster_than_real_time() {
    let mut queue = TestQueue::new(TestQueueOptions {
        ahead_of_time_processing: true,
        ..Default::default()
    });
    let mut input = queue.add_input(
        "input_1",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
        QueueTrackOptions {
            video: true,
            audio: false,
            offset: QueueTrackOffset::FromStart(Duration::ZERO),
        },
    );

    let before_start = Instant::now();
    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    let batches = queue.next_video_batches(6);
    // 120ms worth of output, produced as soon as the input is ready instead of
    // at the real-time pace
    assert!(
        before_start.elapsed() < ms(80),
        "expected output ahead of real time, took {:?}",
        before_start.elapsed()
    );
    assert_video_batches_eq(
        &batches,
        &[
            batch(ms(0), true, frame(0, ms(0))),
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, frame(2, ms(40))),
            batch(ms(60), true, frame(3, ms(60))),
            batch(ms(80), true, frame(4, ms(80))),
            batch(ms(100), true, Eos),
        ],
        EXACT,
    );
}

//
// EOS before any data
//

#[test]
fn eos_before_any_frame_offset_pts() {
    let (mut queue, mut input) = test_queue_with_input(true, QueueTrackOffset::Pts(ms(0)));

    queue.start();
    input.end_video();

    // a closed input doesn't stall the queue even though it's required
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(0), true, Eos)],
        EXACT,
    );
    // no delivered/playing, the input never produced a frame
    queue.expect_events(&[input.video_eos_event()]);
}

#[test]
fn eos_before_any_frame_offset_from_start() {
    let (mut queue, mut input) =
        test_queue_with_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    input.end_video();

    // KNOWN ISSUE (surfaced by this test): with a `FromStart` (or `None`)
    // offset, the track offset is never resolved when the input closes without
    // any data, so `get_frame` bails out before reaching the EOS branch: the
    // input never appears in the output and the EOS event is never emitted.
    // Anything waiting for `VideoInputStreamEos` waits forever.
    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[empty_batch(ms(0)), empty_batch(ms(20))],
        EXACT,
    );
    queue.expect_events(&[]);
    let _ = input;
}

//
// Input/output framerate mismatch
//

#[test]
fn input_framerate_lower_than_output_framerate() {
    let (mut queue, mut input) =
        test_queue_with_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    // 25 fps input into 50 fps output
    input.send_frames(ms(0), ms(40), 3);
    input.end_video();

    // every frame is delivered twice, keeping its original PTS
    assert_video_batches_eq(
        &queue.next_video_batches(6),
        &[
            batch(ms(0), true, frame(0, ms(0))),
            batch(ms(20), true, frame(0, ms(0))),
            batch(ms(40), true, frame(1, ms(40))),
            batch(ms(60), true, frame(1, ms(40))),
            batch(ms(80), true, frame(2, ms(80))),
            batch(ms(100), true, Eos),
        ],
        EXACT,
    );
}

#[test]
fn input_framerate_higher_than_output_framerate() {
    let (mut queue, mut input) =
        test_queue_with_input(true, QueueTrackOffset::FromStart(Duration::ZERO));

    queue.start();
    // 100 fps input into 50 fps output
    input.send_frames(ms(0), ms(10), 10);
    input.end_video();

    // the newest frame not after the batch PTS is delivered, frames in between
    // are dropped; the last frame still goes out in the batch after its PTS
    assert_video_batches_eq(
        &queue.next_video_batches(7),
        &[
            batch(ms(0), true, frame(0, ms(0))),
            batch(ms(20), true, frame(2, ms(20))),
            batch(ms(40), true, frame(4, ms(40))),
            batch(ms(60), true, frame(6, ms(60))),
            batch(ms(80), true, frame(8, ms(80))),
            batch(ms(100), true, frame(9, ms(90))),
            batch(ms(120), true, Eos),
        ],
        EXACT,
    );
}

//
// never_drop_output_frames
//

#[test]
fn never_drop_output_frames_marks_all_batches_required() {
    let mut queue = TestQueue::new(TestQueueOptions {
        never_drop_output_frames: true,
        ..Default::default()
    });
    let mut input = queue.add_input(
        "input_1",
        QueueInputOptions::default(),
        QueueTrackOptions {
            video: true,
            audio: false,
            offset: QueueTrackOffset::FromStart(Duration::ZERO),
        },
    );

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 2);
    input.end_video();

    // the input is optional, but every batch is marked required anyway
    assert_video_batches_eq(
        &queue.next_video_batches(3),
        &[
            batch(ms(0), true, frame(0, ms(0))),
            batch(ms(20), true, frame(1, ms(20))),
            batch(ms(40), true, Eos),
        ],
        EXACT,
    );
}

//
// Slow consumer
//

#[test]
fn slow_consumer_drops_optional_video_batches() {
    let mut queue = TestQueue::new(TestQueueOptions {
        bounded_video_output: true,
        ..Default::default()
    });
    let mut input = queue.add_input(
        "input_1",
        QueueInputOptions::default(),
        QueueTrackOptions {
            video: true,
            audio: false,
            offset: QueueTrackOffset::FromStart(Duration::ZERO),
        },
    );

    queue.start();
    input.send_frames(ms(0), BATCH_DURATION, 5);
    input.end_video();

    // Nobody reads the output: batches of optional inputs are dropped when
    // the consumer doesn't pick them up by their PTS deadline.
    thread::sleep(ms(50));

    // batches 0-2 (and their frames) are gone; consumption resumes mid-stream
    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[
            batch(ms(60), false, frame(3, ms(60))),
            batch(ms(80), false, frame(4, ms(80))),
        ],
        EXACT,
    );
    // EOS batches are required: the queue blocks until the consumer reads them
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(100), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[
        input.video_delivered_event(),
        input.video_playing_event(),
        input.video_eos_event(),
    ]);
}

//
// Scheduled events
//

#[test]
fn scheduled_events_run_in_pts_order() {
    let mut queue = TestQueue::new(TestQueueOptions::default());
    queue.start();

    let (marker_sender, markers) = unbounded();
    // scheduled out of order; PTS are relative to queue start
    let sender = marker_sender.clone();
    queue
        .queue
        .schedule_event(ms(70), Box::new(move || sender.send("second").unwrap()));
    let sender = marker_sender.clone();
    queue
        .queue
        .schedule_event(ms(30), Box::new(move || sender.send("first").unwrap()));

    // An event fires once both video and audio processing pass its PTS, which
    // happens right after the preceding batch is produced.
    assert_video_batches_eq(
        &queue.next_video_batches(3),
        &[empty_batch(ms(0)), empty_batch(ms(20)), empty_batch(ms(40))],
        EXACT,
    );
    assert_eq!(markers.try_iter().collect::<Vec<_>>(), vec!["first"]);

    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[empty_batch(ms(60)), empty_batch(ms(80))],
        EXACT,
    );
    assert_eq!(markers.try_iter().collect::<Vec<_>>(), vec!["second"]);
}

#[test]
fn late_scheduled_event_is_discarded() {
    let mut queue = TestQueue::new(TestQueueOptions::default());
    queue.start();

    assert_video_batches_eq(
        &queue.next_video_batches(3),
        &[empty_batch(ms(0)), empty_batch(ms(20)), empty_batch(ms(40))],
        EXACT,
    );

    // the queue is already past 40ms; an event at 10ms is dropped
    let (marker_sender, markers) = unbounded();
    queue.queue.schedule_event(
        ms(10),
        Box::new(move || marker_sender.send("late").unwrap()),
    );

    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[empty_batch(ms(60)), empty_batch(ms(80))],
        EXACT,
    );
    assert!(markers.try_recv().is_err());
}

#[test]
fn late_scheduled_event_runs_with_run_late_scheduled_events() {
    let mut queue = TestQueue::new(TestQueueOptions {
        run_late_scheduled_events: true,
        ..Default::default()
    });
    queue.start();

    assert_video_batches_eq(
        &queue.next_video_batches(3),
        &[empty_batch(ms(0)), empty_batch(ms(20)), empty_batch(ms(40))],
        EXACT,
    );

    let (marker_sender, markers) = unbounded();
    queue.queue.schedule_event(
        ms(10),
        Box::new(move || marker_sender.send("late").unwrap()),
    );

    // the late event still runs (immediately)
    assert_video_batches_eq(&[queue.next_video_batch()], &[empty_batch(ms(60))], EXACT);
    assert_eq!(markers.try_iter().collect::<Vec<_>>(), vec!["late"]);
}

//
// Input added after start
//

#[test]
fn input_added_after_start() {
    let mut queue = TestQueue::new(TestQueueOptions::default());
    queue.start();

    // the queue produces empty batches while there are no inputs
    assert_video_batches_eq(
        &queue.next_video_batches(3),
        &[empty_batch(ms(0)), empty_batch(ms(20)), empty_batch(ms(40))],
        EXACT,
    );

    // ~40ms after start
    let mut input = queue.add_input(
        "input_1",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
        QueueTrackOptions {
            video: true,
            audio: false,
            offset: QueueTrackOffset::None,
        },
    );
    input.send_frames(ms(0), BATCH_DURATION, 3);
    input.end_video();

    // `None` offset resolves to the first batch processed after the input
    // appeared: the stream plays from 60ms even though its PTS start at 0
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(60), true, frame(0, ms(60)))],
        EXACT,
    );
    queue.expect_events(&[input.video_delivered_event(), input.video_playing_event()]);

    assert_video_batches_eq(
        &queue.next_video_batches(2),
        &[
            batch(ms(80), true, frame(1, ms(80))),
            batch(ms(100), true, frame(2, ms(100))),
        ],
        EXACT,
    );
    assert_video_batches_eq(
        &[queue.next_video_batch()],
        &[batch(ms(120), true, Eos)],
        EXACT,
    );
    queue.expect_events(&[input.video_eos_event()]);
}
