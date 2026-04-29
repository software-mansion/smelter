//! Group 9 — Multi-track queueing.

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
const ZERO: Duration = Duration::ZERO;

fn build() -> QueueHarness {
    QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(true)
        .build()
}

// ---------------------------------------------------------------------------
// 46. Pending track sits idle while current is live.
// ---------------------------------------------------------------------------
#[test]
fn pending_track_idle_while_current_live() {
    let h = build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    // First track: A. Push frames.
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    // Now queue a pending second track (B). Don't push to it.
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.advance(Duration::from_millis(150));
    let r = h.recorded();
    // Outputs should reflect only first track's tags (seq 0 in idx 0).
    let tags = r.video_tags_for_input(0, &a.input_id);
    assert_eq!(tags, vec![(a.input_idx, 0)]);
}

// ---------------------------------------------------------------------------
// 47. Auto-switch on EOS: A drops both senders → B becomes current.
// ---------------------------------------------------------------------------
#[test]
fn auto_switch_on_eos() {
    let h = build();
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
    // Queue track B (also Pts(0) — but offsets are per-track-instance).
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(200)));
    // Push to B (the latest sender stashed in the InputHandle).
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();

    // Track B's first frame (input PTS 0, offset 200ms) lands at output PTS
    // 200ms = output index 6.
    let tags = r.video_tags_for_input(6, &a.input_id);
    // Note: a.input_idx is the same since the InputHandle is the same; we
    // can't visually distinguish A's seq from B's seq using input_idx alone.
    // But we can check that *some* tagged frame appears at idx 6.
    assert!(
        !tags.is_empty(),
        "track B's first frame should appear at output 6"
    );
    assert_eq!(tags, vec![(a.input_idx, 0)]);
}

// ---------------------------------------------------------------------------
// 48. abort_old_track switches mid-stream.
// ---------------------------------------------------------------------------
#[test]
fn abort_old_track_switches() {
    let h = build();
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
    // Queue B.
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(100)));
    a.abort_old_track();
    a.push_video_frame(50, ZERO);
    a.push_video_frame(51, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));
    // After abort, A's remaining frames (1) shouldn't appear; B's frames (seq
    // 50, 51) appear after offset 100ms.
    let r = h.recorded();
    let mut saw_seq_50 = false;
    for buf in &r.video {
        if let Some(PipelineEvent::Data(frame)) = buf.frames.get(&a.input_id)
            && read_video_tag(frame) == (a.input_idx, 50)
        {
            saw_seq_50 = true;
        }
    }
    assert!(saw_seq_50, "track B's seq 50 should surface after abort");
}

// ---------------------------------------------------------------------------
// 49. Two pending tracks consumed FIFO.
// ---------------------------------------------------------------------------
#[test]
fn pending_tracks_fifo() {
    let h = build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_frame(10, ZERO);
    a.drop_video();
    // B
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(100)));
    a.push_video_frame(20, ZERO);
    a.drop_video();
    // C
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(200)));
    a.push_video_frame(30, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(800));
    let r = h.recorded();
    // Find seq 10, 20, 30 — they should appear in that order (by buffer index).
    let mut idx_10 = None;
    let mut idx_20 = None;
    let mut idx_30 = None;
    for (i, buf) in r.video.iter().enumerate() {
        if let Some(PipelineEvent::Data(frame)) = buf.frames.get(&a.input_id) {
            match read_video_tag(frame).1 {
                10 => idx_10.get_or_insert(i),
                20 => idx_20.get_or_insert(i),
                30 => idx_30.get_or_insert(i),
                _ => continue,
            };
        }
    }
    let i10 = idx_10.expect("seq 10");
    let i20 = idx_20.expect("seq 20");
    let i30 = idx_30.expect("seq 30");
    assert!(i10 < i20 && i20 < i30, "FIFO order: {i10}, {i20}, {i30}");
}

// ---------------------------------------------------------------------------
// 50. Pts(d) on new track is honored.
// ---------------------------------------------------------------------------
#[test]
fn new_track_pts_offset_honored() {
    let h = build();
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
    a.queue_video_track(QueueTrackOffset::Pts(Duration::from_millis(500)));
    a.push_video_frame(99, ZERO);
    a.push_video_frame(100, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(800));
    let r = h.recorded();
    // Seq 99 should land at output index 15 (PTS 500ms / 33.3ms ≈ 15).
    let tags = r.video_tags_for_input(15, &a.input_id);
    assert_eq!(tags, vec![(a.input_idx, 99)]);
}
