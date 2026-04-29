//! Group 2 — Video frame selection (older-or-equal logic).
//!
//! Single input unless noted. We drop the sender after pushing so the queue
//! isn't waiting on a newer-frame lookahead. Output framerate is 30fps unless
//! the test explicitly varies it.

use std::time::Duration;

use smelter_render::{Framerate, InputId};

use crate::queue::{
    queue_input::{QueueInputOptions, QueueTrackOffset},
    tests::harness::QueueHarness,
};

const FPS_24: Framerate = Framerate { num: 24, den: 1 };
const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const FPS_60: Framerate = Framerate { num: 60, den: 1 };
const ZERO: Duration = Duration::ZERO;

fn build_30(ahead: bool) -> QueueHarness {
    QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(ahead)
        .build()
}

fn add_required(h: &QueueHarness, name: &str) -> crate::queue::tests::harness::InputHandle {
    h.add_input(
        name,
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    )
}

// Helper: assert that for the first `count` outputs, the tag is `(input_idx, expected_seq[i])`.
fn assert_seq_chain(
    h: &QueueHarness,
    input_id: &InputId,
    input_idx: u32,
    expected_seqs: &[u32],
) {
    h.wait_for_video_count(expected_seqs.len());
    let r = h.recorded();
    for (i, &seq) in expected_seqs.iter().enumerate() {
        let tags = r.video_tags_for_input(i, input_id);
        assert_eq!(
            tags,
            vec![(input_idx, seq)],
            "output buffer {i} expected seq {seq}, got {tags:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 8. Input fps == output fps, same phase.
// ---------------------------------------------------------------------------
#[test]
fn input_30_output_30_same_phase() {
    let h = build_30(true);
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 10, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(500));
    assert_seq_chain(&h, &a.input_id, a.input_idx, &(0..10).collect::<Vec<_>>());
}

// ---------------------------------------------------------------------------
// 9. Input fps == output fps, half-frame offset.
//    Input PTS 16.6, 49.9, … (input_pts = i*33.3 + 16.6). At each output PTS
//    (i*33.3) the older-or-equal frame is the *previous* input frame.
// ---------------------------------------------------------------------------
#[test]
fn input_30_output_30_half_frame_offset() {
    let h = build_30(true);
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    let half = h.frame_interval() / 2;
    let mut ptss = Vec::new();
    for i in 0..10u32 {
        ptss.push(half + h.frame_interval() * i);
    }
    a.push_video_at(&ptss);
    a.drop_video();
    h.advance(Duration::from_millis(700));

    // Output buffer 0 (PTS 0) has no frame older-or-equal (first input is at
    // 16.6ms). The queue waits for at least one frame... but with offset Pts(0)
    // and first-input-pts = 16.6ms, output 0 has nothing. Output 1 (PTS 33.3)
    // sees frame at 16.6ms (older). Output 2 sees frame at 49.9. Etc.
    let r = h.recorded();
    // Output 0: input had no frame <= 0; first delivered output buffer should
    // be index 1 with seq 0.
    let tags_0 = r.video_tags_for_input(0, &a.input_id);
    assert!(
        tags_0.is_empty() || tags_0 == vec![(a.input_idx, 0)],
        "output 0 expected empty or seq 0 (depending on disconnect timing), got {tags_0:?}"
    );
    for (i, expected) in (1..=8).zip(0..8u32) {
        let tags = r.video_tags_for_input(i, &a.input_id);
        assert_eq!(
            tags,
            vec![(a.input_idx, expected)],
            "output {i} expected seq {expected}"
        );
    }
}

// ---------------------------------------------------------------------------
// 10. Input 60 → output 30. Output buffer i should pick frame seq 2*i (since
//     60/30 = 2 and PTS aligns exactly).
// ---------------------------------------------------------------------------
#[test]
fn input_60_output_30() {
    let h = build_30(true);
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_60, 30, ZERO); // 30 frames at 60fps = 500ms
    a.drop_video();
    h.advance(Duration::from_millis(800));

    // Output 0 picks seq 0 (PTS 0 == 0). Output 1 picks seq 2 (PTS 33.3 == 2*16.6).
    // Output i picks seq 2*i. Through 14 outputs we cover seqs 0,2,…,28.
    let expected: Vec<u32> = (0..15).map(|i| i * 2).collect();
    assert_seq_chain(&h, &a.input_id, a.input_idx, &expected);
}

// ---------------------------------------------------------------------------
// 11. Input 24 → output 30. Irregular pairings (older-or-equal):
//     output PTS:  0,    33.3, 66.6, 100,  133.3, 166.6
//     input PTS:   0, 41.66, 83.33, 125,  166.66
//     output seq:  0,    0,    1,    2,    3,    4
// ---------------------------------------------------------------------------
#[test]
fn input_24_output_30_irregular() {
    let h = build_30(true);
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_24, 8, ZERO); // 8 frames covers ~291ms
    a.drop_video();
    h.advance(Duration::from_millis(700));
    // First six output buffers should show seqs [0, 0, 1, 2, 3, 4].
    assert_seq_chain(&h, &a.input_id, a.input_idx, &[0, 0, 1, 2, 3, 4]);
}

// ---------------------------------------------------------------------------
// 12. Input 30 → output 60. Each input frame appears in 2 consecutive outputs.
// ---------------------------------------------------------------------------
#[test]
fn input_30_output_60() {
    let h = QueueHarness::builder()
        .output_framerate(FPS_60)
        .ahead_of_time_processing(true)
        .build();
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 5, ZERO); // 5 frames, 0..132ms
    a.drop_video();
    h.advance(Duration::from_millis(500));

    // Output PTS at 60fps: 0, 16.6, 33.3, 49.9, 66.6, 83.3, 99.9, 116.6, …
    // Input PTS 30fps:    0,        33.3,        66.6,        99.9, …
    // Older-or-equal pairings:
    //   out 0 (0)   → seq 0
    //   out 1 (16.6) → seq 0
    //   out 2 (33.3) → seq 1
    //   out 3 (49.9) → seq 1
    //   out 4 (66.6) → seq 2
    //   out 5 (83.3) → seq 2
    //   out 6 (99.9) → seq 3
    //   out 7 (116.6) → seq 3
    //   out 8 (133.3) → seq 4
    assert_seq_chain(&h, &a.input_id, a.input_idx, &[0, 0, 1, 1, 2, 2, 3, 3, 4]);
}

// ---------------------------------------------------------------------------
// 13. Input fps ≈ output fps but jittered.
//     Input PTS 0, 30ms, 68ms, 99ms, 130ms, 165ms; output 0, 33.3, 66.6, 100,
//     133.3, 166.6.
//     Older-or-equal:
//       out 0 (0)     → seq 0
//       out 1 (33.3)  → seq 1 (30ms is older-or-equal to 33.3)
//       out 2 (66.6)  → seq 1 (68 not yet)... wait actually 30 is older, 68 is newer of 66.6.
//          Hmm: at 66.6, frames 0, 30 are older-or-equal; we pick the latest older-or-equal = 30 (seq 1).
//       out 3 (100)   → seq 3 (99 is older-or-equal to 100 — most recent).
//       out 4 (133.3) → seq 4 (130 is older-or-equal).
//       out 5 (166.6) → seq 5 (165 is older).
// ---------------------------------------------------------------------------
#[test]
fn input_jittered_30() {
    let h = build_30(true);
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    let ms = |n| Duration::from_millis(n);
    a.push_video_at(&[ms(0), ms(30), ms(68), ms(99), ms(130), ms(165)]);
    a.drop_video();
    h.advance(Duration::from_millis(700));
    assert_seq_chain(&h, &a.input_id, a.input_idx, &[0, 1, 1, 3, 4, 5]);
}

// ---------------------------------------------------------------------------
// 14. Frame held until newer arrives. Two frames at large PTS gap; output
//     buffers between them all carry the older one.
// ---------------------------------------------------------------------------
#[test]
fn frame_held_until_newer() {
    let h = build_30(true);
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, Duration::from_millis(200)); // 200ms gap
    a.drop_video();
    h.advance(Duration::from_millis(500));

    let r = h.recorded();
    // At 30fps, output buffers 0..5 (PTS 0..166ms) should all contain seq 0.
    // Output 6 (PTS 200ms) should contain seq 1.
    for i in 0..6 {
        let tags = r.video_tags_for_input(i, &a.input_id);
        assert_eq!(
            tags,
            vec![(a.input_idx, 0)],
            "output {i} held seq 0"
        );
    }
    let tags_6 = r.video_tags_for_input(6, &a.input_id);
    assert_eq!(tags_6, vec![(a.input_idx, 1)]);
}

// ---------------------------------------------------------------------------
// 15. No frame ≤ pts → queue waits. Then a frame arrives and outputs back-fill.
// ---------------------------------------------------------------------------
#[test]
fn no_frame_yet_queue_waits_then_backfills() {
    let h = build_30(true);
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    h.advance(Duration::from_millis(200));
    // Nothing pushed yet; recorder is empty.
    {
        let r = h.recorded();
        assert!(r.video.is_empty(), "no outputs before first frame");
    }
    // Push at PTS 0 — should backfill output buffer 0.
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));

    let r = h.recorded();
    assert!(!r.video.is_empty(), "outputs should appear after frames pushed");
    let tags_0 = r.video_tags_for_input(0, &a.input_id);
    assert_eq!(tags_0, vec![(a.input_idx, 0)]);
}

// ---------------------------------------------------------------------------
// 16. Last-frame-on-disconnect popped exactly once. After dropping the
//     sender, the single remaining frame surfaces in exactly one buffer
//     followed by EOS.
// ---------------------------------------------------------------------------
#[test]
fn last_frame_on_disconnect_popped_once() {
    let h = build_30(true);
    let a = add_required(&h, "a");
    a.queue_video_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_frame(0, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(200));

    let r = h.recorded();
    let mut seen_seq0 = 0;
    let mut seen_eos = 0;
    for buf in &r.video {
        if let Some(ev) = buf.frames.get(&a.input_id) {
            match ev {
                crate::prelude::PipelineEvent::Data(_) => seen_seq0 += 1,
                crate::prelude::PipelineEvent::EOS => seen_eos += 1,
            }
        }
    }
    assert_eq!(seen_seq0, 1, "frame seq 0 should appear in exactly 1 buffer");
    assert_eq!(seen_eos, 1, "EOS should appear exactly once");
}
