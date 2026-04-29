//! Group 5 — `required`, `never_drop`, slow consumer.
//!
//! Timing tests. `start_with_consumer_lag` puts a sleep before each recv on
//! both consumer threads, simulating slow downstream.

use std::time::Duration;

use smelter_render::Framerate;

use crate::queue::{
    queue_input::{QueueInputOptions, QueueTrackOffset},
    tests::harness::QueueHarness,
};

const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const ZERO: Duration = Duration::ZERO;

// ---------------------------------------------------------------------------
// 28. Non-required + slow consumer + ahead_of_time=true.
//     Some output buffers should drop (deadline expiry on send_deadline).
// ---------------------------------------------------------------------------
#[test]
fn non_required_slow_consumer_drops() {
    let h = QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(true)
        .build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: false,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start_with_consumer_lag(Duration::from_millis(50));
    a.push_video_stream(FPS_30, 30, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(2000));
    let r = h.recorded();
    // We pushed 30 frames at 30fps; with 50ms consumer lag and the queue
    // generating outputs ahead of the wall clock, some should drop. We
    // observe that the recorded count is strictly less than the frames we
    // would have produced if nothing dropped.
    assert!(
        r.video.len() < 30,
        "expected drops; recorded {} buffers",
        r.video.len()
    );
}

// ---------------------------------------------------------------------------
// 29. Required + slow consumer: queue blocks on send; advance time without
//     consumer drain → very few outputs progress.
// ---------------------------------------------------------------------------
#[test]
fn required_slow_consumer_blocks() {
    let h = QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(true)
        .build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start_with_consumer_lag(Duration::from_millis(100));
    a.push_video_stream(FPS_30, 10, ZERO);
    a.drop_video();
    // Don't advance much; lag dominates.
    h.advance(Duration::from_millis(100));
    let r = h.recorded();
    // Required → no deadline-based drops. Few outputs delivered yet because
    // consumer is slow and queue blocks on send.
    assert!(
        r.video.len() <= 3,
        "queue should be blocked by slow consumer, got {}",
        r.video.len()
    );
}

// ---------------------------------------------------------------------------
// 30. never_drop_output_frames=true upgrades non-required input.
// ---------------------------------------------------------------------------
#[test]
fn never_drop_upgrades_non_required() {
    let h = QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(true)
        .never_drop_output_frames(true)
        .build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: false,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start_with_consumer_lag(Duration::from_millis(30));
    a.push_video_stream(FPS_30, 10, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(2000));
    h.wait_for_video_count(10);
    let r = h.recorded();
    assert!(
        r.video.len() >= 10,
        "never_drop should not drop, got {}",
        r.video.len()
    );
}

// ---------------------------------------------------------------------------
// 31. EOS always required, even with non-required input + flag false.
// ---------------------------------------------------------------------------
#[test]
fn eos_always_required() {
    let h = QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(true)
        .never_drop_output_frames(false)
        .build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: false,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start_with_consumer_lag(Duration::from_millis(80));
    a.push_video_frame(0, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(500));
    h.wait_for_video_count(1);
    let r = h.recorded();
    let mut saw_eos = false;
    for buf in &r.video {
        if let Some(crate::prelude::PipelineEvent::EOS) = buf.frames.get(&a.input_id) {
            assert!(buf.required, "EOS-carrying buffer must be required");
            saw_eos = true;
        }
    }
    assert!(saw_eos, "EOS should be delivered");
}

// ---------------------------------------------------------------------------
// 32. Required + ahead_of_time=false: queue paces to wall clock; required
//     doesn't busy-loop.
// ---------------------------------------------------------------------------
#[test]
fn required_with_pacing_does_not_busy_loop() {
    let h = QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(false)
        .build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 15, ZERO);
    a.drop_video();
    // Don't advance the clock — queue should not produce outputs ahead of time.
    h.flush();
    let count_immediately = h.recorded().video.len();
    // With pacing, very few outputs should have been delivered without clock
    // advancement (only those whose PTS <= 0).
    assert!(
        count_immediately <= 1,
        "without clock advancement, queue should not race ahead, got {count_immediately}"
    );
    // Now advance clock and outputs flow.
    h.advance(Duration::from_millis(600));
    h.wait_for_video_count(10);
}
