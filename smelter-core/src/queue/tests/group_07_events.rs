//! Group 7 — Scheduled events.

use std::time::Duration;

use smelter_render::Framerate;

use crate::queue::{
    queue_input::{QueueInputOptions, QueueTrackOffset},
    tests::harness::QueueHarness,
};

const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const ZERO: Duration = Duration::ZERO;

fn build_with_late_flag(late: bool) -> QueueHarness {
    QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(true)
        .run_late_scheduled_events(late)
        .build()
}

// ---------------------------------------------------------------------------
// 35. Event at PTS T fires after both video & audio cross T.
// ---------------------------------------------------------------------------
#[test]
fn event_fires_after_both_streams_cross_pts() {
    let h = build_with_late_flag(false);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.schedule_event(Duration::from_millis(100), 1);
    // Video advances quickly; audio more slowly. Push both to ~150ms.
    a.push_video_stream(FPS_30, 6, ZERO);
    a.drop_video();
    a.push_audio_stream(Duration::from_millis(20), 8, ZERO);
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    assert_eq!(r.events_fired, vec![1], "event should fire once");
}

// ---------------------------------------------------------------------------
// 36. Multiple events at same PTS all fire.
// ---------------------------------------------------------------------------
#[test]
fn multiple_events_same_pts() {
    let h = build_with_late_flag(false);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.schedule_event(Duration::from_millis(50), 10);
    h.schedule_event(Duration::from_millis(50), 11);
    h.schedule_event(Duration::from_millis(50), 12);
    a.push_video_stream(FPS_30, 4, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(300));
    let r = h.recorded();
    let mut fired = r.events_fired.clone();
    fired.sort();
    assert_eq!(fired, vec![10, 11, 12]);
}

// ---------------------------------------------------------------------------
// 37. Late event with run_late=false is dropped.
// ---------------------------------------------------------------------------
#[test]
fn late_event_dropped_when_flag_false() {
    let h = build_with_late_flag(false);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    // Push so queue PTS advances past 50ms.
    a.push_video_stream(FPS_30, 6, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 8, ZERO);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(300));
    // Queue is now well past 50ms. Schedule a late event.
    h.schedule_event(Duration::from_millis(50), 99);
    h.advance(Duration::from_millis(200));
    let r = h.recorded();
    assert!(
        !r.events_fired.contains(&99),
        "late event should be dropped when run_late=false"
    );
}

// ---------------------------------------------------------------------------
// 38. Late event with run_late=true is invoked.
// ---------------------------------------------------------------------------
#[test]
fn late_event_invoked_when_flag_true() {
    let h = build_with_late_flag(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 6, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 8, ZERO);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(300));
    h.schedule_event(Duration::from_millis(50), 99);
    h.advance(Duration::from_millis(200));
    let r = h.recorded();
    assert!(
        r.events_fired.contains(&99),
        "late event should fire when run_late=true"
    );
}

// ---------------------------------------------------------------------------
// 39. Event ordering monotonic in PTS even when scheduled out of order.
// ---------------------------------------------------------------------------
#[test]
fn event_ordering_monotonic() {
    let h = build_with_late_flag(false);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.schedule_event(Duration::from_millis(150), 3);
    h.schedule_event(Duration::from_millis(50), 1);
    h.schedule_event(Duration::from_millis(100), 2);
    a.push_video_stream(FPS_30, 8, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 12, ZERO);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    assert_eq!(r.events_fired, vec![1, 2, 3]);
}
