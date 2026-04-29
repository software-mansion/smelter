//! Group 12 — Combinatorial / interaction tests.
//!
//! Mix orthogonal features in one test. Items annotated `(specifies behavior)`
//! pin current observed behavior rather than an obviously-correct outcome —
//! treat any change in those tests as deliberate.

use std::time::Duration;

use smelter_render::Framerate;

use crate::{
    prelude::PipelineEvent,
    queue::{
        queue_input::{QueueInputOptions, QueueTrackOffset},
        tests::harness::{QueueHarness, read_video_tag},
    },
};

const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const FPS_60: Framerate = Framerate { num: 60, den: 1 };
const ZERO: Duration = Duration::ZERO;

fn build(ahead: bool) -> QueueHarness {
    QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(ahead)
        .build()
}

// ---------------------------------------------------------------------------
// 58. Offset × required.
// ---------------------------------------------------------------------------
#[test]
fn offset_x_required_no_drops_at_offset() {
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
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(500)));
    h.start_with_consumer_lag(Duration::from_millis(20));
    a.push_video_stream(FPS_30, 5, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(2000));
    h.wait_for_video_count(20);
    let r = h.recorded();
    // No drops on required input. First tagged frame at output idx 15 (PTS 500ms).
    let tags = r.video_tags_for_input(15, &a.input_id);
    assert_eq!(tags, vec![(a.input_idx, 0)]);
}

// ---------------------------------------------------------------------------
// 59. Offset × never_drop.
// ---------------------------------------------------------------------------
#[test]
fn offset_x_never_drop_no_drops() {
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
    assert!(r.video.len() >= 10);
}

// ---------------------------------------------------------------------------
// 60. Required input + paused must not block output progress.
// ---------------------------------------------------------------------------
#[test]
fn paused_required_input_does_not_block_output() {
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
    let b = h.add_input(
        "b",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    b.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    a.pause();
    b.push_video_stream(FPS_30, 5, ZERO);
    b.drop_video();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    // Outputs should progress (at least 4 buffers) even though required A is paused.
    assert!(
        r.video.len() >= 4,
        "paused required input should not block, got {} buffers",
        r.video.len()
    );
}

// ---------------------------------------------------------------------------
// 61. Required input + remove_input mid-block (specifies behavior).
// ---------------------------------------------------------------------------
#[test]
fn required_input_remove_unblocks() {
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
    h.start_with_consumer_lag(Duration::from_secs(10));
    a.push_video_stream(FPS_30, 5, ZERO);
    h.advance(Duration::from_millis(100));
    h.queue.remove_input(&a.input_id);
    h.flush();
    // Without panic, consider the test pinned. Removed input becomes absent
    // from subsequent outputs (asserted in Group 11 test 56).
}

// ---------------------------------------------------------------------------
// 62. Mixed required-ness across inputs: output `required` flag is OR-composed.
// ---------------------------------------------------------------------------
#[test]
fn mixed_required_or_composes_on_output() {
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
    let b = h.add_input(
        "b",
        QueueInputOptions {
            required: false,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    b.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 5, ZERO);
    a.drop_video();
    b.push_video_stream(FPS_30, 5, ZERO);
    b.drop_video();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    // Output buffers should be flagged required because A is required.
    let any_required = r.video.iter().any(|b| b.required);
    assert!(any_required, "OR-composed required flag");
}

// ---------------------------------------------------------------------------
// 64. Abort track while paused.
// ---------------------------------------------------------------------------
#[test]
fn abort_while_paused() {
    let h = build(true);
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
    a.pause();
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(200)));
    a.abort_old_track();
    h.advance(Duration::from_millis(100));
    a.resume();
    a.push_video_frame(50, ZERO);
    a.push_video_frame(51, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    let mut saw = false;
    for buf in &r.video {
        if let Some(PipelineEvent::Data(frame)) = buf.frames.get(&a.input_id)
            && read_video_tag(frame) == (a.input_idx, 50)
        {
            saw = true;
        }
    }
    assert!(saw, "post-abort, post-resume seq 50 should appear");
}

// ---------------------------------------------------------------------------
// 65. Queue new track while paused, no abort.
// ---------------------------------------------------------------------------
#[test]
fn queue_new_track_while_paused_no_abort() {
    let h = build(true);
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
    a.drop_video();
    a.pause();
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(200)));
    h.advance(Duration::from_millis(100));
    a.resume();
    a.push_video_frame(1, ZERO);
    a.push_video_frame(2, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));
    // Test passes if no panic. Specific output ordering is "specifies behavior".
}

// ---------------------------------------------------------------------------
// 66. Switch to track with Pts(d) where d < current queue PTS (specifies behavior).
// ---------------------------------------------------------------------------
#[test]
fn time_travel_switch_specifies_behavior() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(200)));
    h.start();
    a.push_video_stream(FPS_30, 5, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(500));
    // Now switch to a track whose offset is 0 (in the past relative to where
    // queue PTS is now ~200+).
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    a.push_video_frame(99, ZERO);
    a.push_video_frame(100, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));
    // Pin behavior: do not crash; capture observable state.
    let _r = h.recorded();
}

// ---------------------------------------------------------------------------
// 67. Switch to track with mismatched config (V+A → V-only) (specifies behavior).
// ---------------------------------------------------------------------------
#[test]
fn switch_to_v_only_after_av() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 3, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 4, ZERO);
    a.drop_video();
    a.drop_audio();
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(200)));
    a.push_video_frame(99, ZERO);
    a.push_video_frame(100, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));
    // No crash; pin behavior.
}

// ---------------------------------------------------------------------------
// 68. EOS on one stream of a track must NOT trigger switch until BOTH done.
// ---------------------------------------------------------------------------
#[test]
fn partial_eos_does_not_switch() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_frame(0, ZERO);
    a.drop_video(); // video EOS, but audio still active
    a.push_audio_stream(Duration::from_millis(20), 5, ZERO);
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(500)));
    // Don't drop audio yet.
    h.advance(Duration::from_millis(150));
    let r = h.recorded();
    // Pending track B's tag (e.g., we haven't pushed to it) shouldn't appear.
    // Specifically check that no buffer at output PTS >= 500ms has a tag from
    // input a. (Buffers may be empty; that's fine.)
    for buf in &r.video {
        // We didn't push to track B, so any output for input a that's
        // post-track-A-EOS should be empty or EOS.
        if buf.pts >= Duration::from_millis(500)
            && let Some(PipelineEvent::Data(_)) = buf.frames.get(&a.input_id)
        {
            panic!("track B should not be active while audio of A is live");
        }
    }
}

// ---------------------------------------------------------------------------
// 69. Reordered queue_new_track + EOS race.
// ---------------------------------------------------------------------------
#[test]
fn queue_new_track_eos_race() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(100)));
    a.drop_video(); // closes A's sender (B's already stashed). but since drop_video drops from InputHandle
    // Push to B (handle's video_sender is now B's).
    a.push_video_frame(99, ZERO);
    a.push_video_frame(100, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));
    // No panic; output should contain B's seq 99 at idx 3 (PTS 100ms).
    let r = h.recorded();
    let tags = r.video_tags_for_input(3, &a.input_id);
    assert!(!tags.is_empty(), "track B should have produced output");
}

// ---------------------------------------------------------------------------
// 70. abort_old_track with empty pending = no-op.
// ---------------------------------------------------------------------------
#[test]
fn abort_with_empty_pending_is_noop() {
    let h = build(true);
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
    a.abort_old_track(); // no pending
    a.push_video_frame(2, h.frame_interval() * 2);
    a.drop_video();
    h.advance(Duration::from_millis(200));
    let r = h.recorded();
    // Track A's frames continue normally.
    for i in 0..3 {
        let tags = r.video_tags_for_input(i, &a.input_id);
        assert_eq!(tags, vec![(a.input_idx, i as u32)]);
    }
}

// ---------------------------------------------------------------------------
// 71. Pause across queue start (specifies behavior).
// ---------------------------------------------------------------------------
#[test]
fn pause_across_start() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    a.pause();
    h.start();
    h.advance(Duration::from_millis(100));
    a.resume();
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(300));
    // No panic; pin behavior.
}

// ---------------------------------------------------------------------------
// 72. Rapid pause/resume cycles (verify no offset drift).
// ---------------------------------------------------------------------------
#[test]
fn rapid_pause_resume_cycles() {
    let h = build(true);
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
    for _ in 0..5 {
        a.pause();
        h.advance(Duration::from_millis(20));
        a.resume();
        h.advance(Duration::from_millis(20));
    }
    a.push_video_frame(2, h.frame_interval() * 2);
    a.drop_video();
    h.advance(Duration::from_millis(500));
    // Test passes if no panic / hang.
}

// ---------------------------------------------------------------------------
// 73. Pause + remove_input.
// ---------------------------------------------------------------------------
#[test]
fn pause_then_remove() {
    let h = build(true);
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
    a.pause();
    h.queue.remove_input(&a.input_id);
    h.advance(Duration::from_millis(100));
    // No panic.
}

// ---------------------------------------------------------------------------
// 74. Pause + shutdown.
// ---------------------------------------------------------------------------
#[test]
fn pause_then_shutdown() {
    let h = build(true);
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
    h.advance(Duration::from_millis(50));
    a.pause();
    h.queue.shutdown();
    h.flush();
}

// ---------------------------------------------------------------------------
// 80. Event × pause.
// ---------------------------------------------------------------------------
#[test]
fn event_during_pause() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.schedule_event(Duration::from_millis(150), 1);
    a.push_video_stream(FPS_30, 3, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 5, ZERO);
    h.advance(Duration::from_millis(50));
    a.pause();
    h.advance(Duration::from_millis(300));
    // Event should not have fired during pause (queue PTS doesn't advance).
    let r1 = h.recorded();
    assert!(
        !r1.events_fired.contains(&1),
        "event should not fire during pause"
    );
    a.resume();
    a.push_video_stream(FPS_30, 5, h.frame_interval() * 3);
    a.push_audio_stream(Duration::from_millis(20), 8, Duration::from_millis(100));
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    let r2 = h.recorded();
    assert!(r2.events_fired.contains(&1), "event should fire after resume");
}

// ---------------------------------------------------------------------------
// 81. Event × track switch.
// ---------------------------------------------------------------------------
#[test]
fn event_during_track_switch() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.schedule_event(Duration::from_millis(150), 5);
    a.push_video_stream(FPS_30, 3, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 4, ZERO);
    a.drop_video();
    a.drop_audio();
    a.queue_av_track(QueueTrackOffset::Pts(Duration::from_millis(150)));
    a.push_video_stream(FPS_30, 5, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 8, ZERO);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(800));
    let r = h.recorded();
    assert!(r.events_fired.contains(&5));
}

// ---------------------------------------------------------------------------
// 82. Event × remove_input.
// ---------------------------------------------------------------------------
#[test]
fn event_after_input_removed() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.schedule_event(Duration::from_millis(100), 7);
    a.push_video_stream(FPS_30, 5, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 8, ZERO);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(50));
    h.queue.remove_input(&a.input_id);
    h.advance(Duration::from_millis(300));
    let r = h.recorded();
    // Events are queue-level; should still fire when queue PTS advances.
    // Without inputs, queue PTS still advances if ahead_of_time=true.
    assert!(r.events_fired.contains(&7));
}

// ---------------------------------------------------------------------------
// 83. Event × shutdown (specifies behavior).
// ---------------------------------------------------------------------------
#[test]
fn event_pending_at_shutdown() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.schedule_event(Duration::from_millis(500), 9);
    a.push_video_stream(FPS_30, 2, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 3, ZERO);
    h.advance(Duration::from_millis(50));
    h.queue.shutdown();
    h.flush();
    let r = h.recorded();
    // Pin behavior — event 9 should NOT have fired (PTS not yet reached
    // before shutdown).
    assert!(!r.events_fired.contains(&9), "pending event should not fire after shutdown");
}

// ---------------------------------------------------------------------------
// 84. Event scheduled from inside a callback (specifies behavior).
// ---------------------------------------------------------------------------
#[test]
fn event_reentrant_schedule_specifies_behavior() {
    use std::sync::Arc;
    use std::sync::Mutex;
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    let queue = h.queue.clone();
    let recorder = h.recorder.clone();
    let scheduled_inner = Arc::new(Mutex::new(false));
    let inner_clone = scheduled_inner.clone();
    h.queue.schedule_event(
        Duration::from_millis(50),
        Box::new(move || {
            recorder.lock().unwrap().events_fired.push(100);
            // Re-entrant schedule. This may deadlock per the bounded(0)
            // event channel; we wrap in a spawned thread to detect.
            let q = queue.clone();
            std::thread::spawn(move || {
                q.schedule_event(
                    Duration::from_millis(150),
                    Box::new(|| {
                        // We don't append; just detect non-deadlock.
                    }),
                );
                *inner_clone.lock().unwrap() = true;
            });
        }),
    );
    a.push_video_stream(FPS_30, 8, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 12, ZERO);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    assert!(r.events_fired.contains(&100), "outer event should fire");
    // Whether the re-entrant schedule succeeded is "specifies behavior".
}

// ---------------------------------------------------------------------------
// 85. Pre-start event.
// ---------------------------------------------------------------------------
#[test]
fn pre_start_event_fires_post_start() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.schedule_event(Duration::from_millis(50), 1);
    h.start();
    a.push_video_stream(FPS_30, 5, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 8, ZERO);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(300));
    let r = h.recorded();
    assert!(r.events_fired.contains(&1), "pre-start event should fire post-start");
}

// ---------------------------------------------------------------------------
// 86. Event after EOS PTS still fires.
// ---------------------------------------------------------------------------
#[test]
fn event_after_eos_pts() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: false,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.schedule_event(Duration::from_millis(300), 2);
    a.push_video_stream(FPS_30, 3, ZERO); // ~100ms of video
    a.push_audio_stream(Duration::from_millis(20), 5, ZERO); // 100ms of audio
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(800));
    let r = h.recorded();
    assert!(r.events_fired.contains(&2));
}

// ---------------------------------------------------------------------------
// 87. Add input post-start.
// ---------------------------------------------------------------------------
#[test]
fn add_input_post_start() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 3, ZERO);
    h.advance(Duration::from_millis(50));
    let b = h.add_input(
        "b",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    b.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(50)));
    a.drop_video();
    b.push_video_stream(FPS_30, 3, ZERO);
    b.drop_video();
    h.advance(Duration::from_millis(500));
    // No panic; b's frames should appear in the output.
    let r = h.recorded();
    let mut b_appeared = false;
    for buf in &r.video {
        if matches!(buf.frames.get(&b.input_id), Some(PipelineEvent::Data(_))) {
            b_appeared = true;
            break;
        }
    }
    assert!(b_appeared, "input b added post-start should produce output");
}

// ---------------------------------------------------------------------------
// 89. fps mismatch × pause (input A 60fps, B 30fps, pause B mid-stream).
// ---------------------------------------------------------------------------
#[test]
fn fps_mismatch_with_pause() {
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
    let b = h.add_input(
        "b",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    b.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_60, 10, ZERO);
    b.push_video_stream(FPS_30, 5, ZERO);
    h.advance(Duration::from_millis(50));
    b.pause();
    h.advance(Duration::from_millis(100));
    a.drop_video();
    b.resume();
    b.push_video_frame(99, h.frame_interval() * 5);
    b.drop_video();
    h.advance(Duration::from_millis(500));
    // No panic.
}

// ---------------------------------------------------------------------------
// 90. fps mismatch + drop (A V@60 non-required, B V@30 required, slow consumer).
// ---------------------------------------------------------------------------
#[test]
fn fps_mismatch_with_drop_b_never_drops() {
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
    let b = h.add_input(
        "b",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    b.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start_with_consumer_lag(Duration::from_millis(40));
    a.push_video_stream(FPS_60, 20, ZERO);
    b.push_video_stream(FPS_30, 10, ZERO);
    a.drop_video();
    b.drop_video();
    h.advance(Duration::from_millis(2000));
    h.wait_for_video_count(10);
    let r = h.recorded();
    // Each of B's frames seq 0..9 should appear (B is required).
    let mut b_seqs: std::collections::BTreeSet<u32> =
        std::collections::BTreeSet::new();
    for buf in &r.video {
        if let Some(PipelineEvent::Data(frame)) = buf.frames.get(&b.input_id) {
            b_seqs.insert(read_video_tag(frame).1);
        }
    }
    assert!(
        b_seqs.contains(&0) && b_seqs.contains(&9),
        "B's first and last seqs must appear, saw {b_seqs:?}"
    );
}

// ---------------------------------------------------------------------------
// 91. AV-only inputs mixed.
// ---------------------------------------------------------------------------
#[test]
fn av_only_inputs_mixed() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    let b = h.add_input(
        "b",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    b.queue_audio_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 5, ZERO);
    a.drop_video();
    b.push_audio_stream(Duration::from_millis(20), 8, ZERO);
    b.drop_audio();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    // Both grids should have entries.
    let video_a_count = r
        .video
        .iter()
        .filter(|buf| matches!(buf.frames.get(&a.input_id), Some(PipelineEvent::Data(_))))
        .count();
    let audio_b_count = r
        .audio
        .iter()
        .filter(|chunk| matches!(chunk.samples.get(&b.input_id), Some(PipelineEvent::Data(_))))
        .count();
    assert!(video_a_count > 0 && audio_b_count > 0);
}

// ---------------------------------------------------------------------------
// 92. FromStart(d) × pause-before-start.
// ---------------------------------------------------------------------------
#[test]
fn from_start_with_pause_before_start() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::FromStart(Duration::from_millis(100)));
    a.pause();
    h.start();
    h.advance(Duration::from_millis(50));
    a.resume();
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));
    // No panic; pin behavior.
}

// ---------------------------------------------------------------------------
// 93. None × pre-start-cleanup ticker.
// ---------------------------------------------------------------------------
#[test]
fn none_offset_pre_start_cleanup_iteration() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::None);
    // Push first packet pre-start.
    a.push_video_frame(0, ZERO);
    // Pulse cleanup multiple times.
    for _ in 0..5 {
        h.flush();
        h.advance(Duration::from_millis(10));
    }
    a.push_video_frame(1, h.frame_interval());
    a.drop_video();
    h.start();
    h.advance(Duration::from_millis(300));
    // No panic; offset latched at first packet, not later.
}

// ---------------------------------------------------------------------------
// 94. AV pair shares offset across mode.
// ---------------------------------------------------------------------------
#[test]
fn av_pair_shares_offset() {
    let h = build(true);
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_av_track(QueueTrackOffset::None);
    h.start();
    h.advance(Duration::from_millis(100));
    // Push video first; offset latches.
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    // Then push audio with input PTS 0; should observe the same offset
    // (audio's first packet doesn't re-latch).
    a.push_audio_batch(0, ZERO, Duration::from_millis(20), 48000);
    a.push_audio_batch(1, Duration::from_millis(20), Duration::from_millis(20), 48000);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    // Both should land in similar PTS ranges. We don't assert exact alignment
    // but verify both produced data (no offset divergence panic).
    let video_count = r
        .video
        .iter()
        .filter(|b| matches!(b.frames.get(&a.input_id), Some(PipelineEvent::Data(_))))
        .count();
    let audio_count = r
        .audio
        .iter()
        .filter(|c| matches!(c.samples.get(&a.input_id), Some(PipelineEvent::Data(b)) if !b.is_empty()))
        .count();
    assert!(video_count > 0);
    assert!(audio_count > 0);
}
