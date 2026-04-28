//! Group 11 — Lifecycle.

use std::time::Duration;

use smelter_render::Framerate;

use crate::queue::{
    queue_input::{QueueInputOptions, QueueTrackOffset},
    tests::harness::QueueHarness,
};

const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const ZERO: Duration = Duration::ZERO;

// ---------------------------------------------------------------------------
// 55. shutdown exits the queue thread.
// ---------------------------------------------------------------------------
#[test]
fn shutdown_exits_queue_thread() {
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
    h.start();
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    h.advance(Duration::from_millis(50));
    h.queue.shutdown();
    h.flush();
    // Queue thread should have set should_close and exited; subsequent
    // pulses don't crash. We can't easily observe thread death from outside,
    // so verify by ensuring the harness Drop completes cleanly (test exits
    // normally).
}

// ---------------------------------------------------------------------------
// 56. remove_input mid-flight: in-flight frames don't appear post-removal.
// ---------------------------------------------------------------------------
#[test]
fn remove_input_mid_flight() {
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
    h.start();
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    h.advance(Duration::from_millis(50));
    let count_before = h.recorded().video.len();
    h.queue.remove_input(&a.input_id);
    // Push more frames — should not appear because input was removed.
    a.push_video_frame(2, h.frame_interval() * 2);
    a.push_video_frame(3, h.frame_interval() * 3);
    h.advance(Duration::from_millis(200));
    let r = h.recorded();
    // Output buffers after removal should not contain entries for a.input_id.
    for buf in &r.video[count_before..] {
        assert!(
            !buf.frames.contains_key(&a.input_id),
            "removed input should not appear in subsequent outputs"
        );
    }
}

// ---------------------------------------------------------------------------
// 57. Shutdown while required-blocked: pins current behavior. The queue
//     thread is in a blocking `send()` call when required=true and the
//     consumer is stalled. shutdown sets the should_close flag, but the
//     blocking send won't unblock until a recv occurs — that's the documented
//     hazard at queue_thread.rs:262-264.
// ---------------------------------------------------------------------------
#[test]
fn shutdown_while_required_blocked() {
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
    // Start with very slow consumer so queue blocks on send.
    h.start_with_consumer_lag(Duration::from_secs(60));
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    h.advance(Duration::from_millis(100));
    // Shutdown — pins behavior. We don't assert that the queue thread
    // immediately exits because the documented behavior is that it may
    // remain blocked in send() until consumer drains. Just verify shutdown
    // doesn't panic.
    h.queue.shutdown();
    // Let test scope end so harness Drop runs.
}
