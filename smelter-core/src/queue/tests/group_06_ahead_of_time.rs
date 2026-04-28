//! Group 6 — `ahead_of_time_processing`.

use std::time::Duration;

use smelter_render::Framerate;

use crate::queue::{
    queue_input::{QueueInputOptions, QueueTrackOffset},
    tests::harness::QueueHarness,
};

const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const ZERO: Duration = Duration::ZERO;

// ---------------------------------------------------------------------------
// 33. ahead_of_time=false: queue waits for clock.
// ---------------------------------------------------------------------------
#[test]
fn paced_to_clock_no_advance_no_output() {
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
    a.push_video_stream(FPS_30, 30, ZERO);
    a.drop_video();
    // Without clock advance, queue should produce ~0 buffers.
    h.flush();
    let r0 = h.recorded();
    assert!(
        r0.video.len() <= 1,
        "without clock advance, expected <=1 output, got {}",
        r0.video.len()
    );

    // Advance 100ms → about 3 frames worth.
    h.advance(Duration::from_millis(100));
    let r1 = h.recorded();
    assert!(
        r1.video.len() >= 3 && r1.video.len() <= 5,
        "after 100ms expected ~3-5 outputs, got {}",
        r1.video.len()
    );

    // Advance fully.
    h.advance(Duration::from_millis(900));
    h.wait_for_video_count(30);
}

// ---------------------------------------------------------------------------
// 34. ahead_of_time=true: queue emits as soon as data is available.
// ---------------------------------------------------------------------------
#[test]
fn ahead_of_time_emits_immediately() {
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
    a.push_video_stream(FPS_30, 30, ZERO);
    a.drop_video();
    // No clock advance, but data is all there → queue produces all outputs.
    h.flush();
    h.wait_for_video_count(30);
    let r = h.recorded();
    assert!(
        r.video.len() >= 30,
        "ahead_of_time should produce all outputs immediately, got {}",
        r.video.len()
    );
}
