//! Group 1 — Offset semantics.
//!
//! Each test has one input, a short stream, and asserts where frames land in
//! the output PTS grid. Identity-style: we read `(input_idx, seq)` tags off
//! each output frame.

use std::time::Duration;

use smelter_render::Framerate;

use crate::queue::{
    queue_input::{QueueInputOptions, QueueTrackOffset},
    tests::harness::QueueHarness,
};

const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const ZERO: Duration = Duration::ZERO;

/// `push count` frames at the input's frame interval and let the queue catch
/// up, then assert each output buffer i contains tag `(input_idx, i)`.
fn assert_identity_chain(h: &QueueHarness, input_idx: u32, input_id: &smelter_render::InputId, count: usize) {
    h.wait_for_video_count(count);
    let r = h.recorded();
    assert!(
        r.video.len() >= count,
        "expected at least {count} video buffers, got {}",
        r.video.len()
    );
    for i in 0..count {
        let tags = r.video_tags_for_input(i, input_id);
        assert_eq!(
            tags,
            vec![(input_idx, i as u32)],
            "output buffer {i} mismatch"
        );
    }
}

// ---------------------------------------------------------------------------
// 0. Sanity: harness setup yields the first output buffer after pushing 2
//    frames (the queue uses a two-frame lookahead to confirm older-or-equal
//    selection).
// ---------------------------------------------------------------------------
#[test]
fn sanity_two_frames_yields_first_output() {
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
    h.advance(Duration::from_millis(100));
    let r = h.recorded();
    assert!(
        r.video.len() >= 1,
        "expected at least 1 output buffer, got {}",
        r.video.len()
    );
    let tags = r.video_tags_for_input(0, &a.input_id);
    assert_eq!(tags, vec![(a.input_idx, 0)]);
}

// ---------------------------------------------------------------------------
// 1. Pts(0) realtime alignment.
//    Input frame seq i (PTS i/30s) → output buffer i.
// ---------------------------------------------------------------------------
#[test]
fn pts_zero_realtime_alignment() {
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
    a.push_video_stream(FPS_30, 10, ZERO);
    // Drop sender so the last frame can surface (otherwise the queue's
    // two-frame lookahead would wait for an 11th frame).
    a.drop_video();
    h.advance(Duration::from_millis(400));

    assert_identity_chain(&h, a.input_idx, &a.input_id, 10);
}

// ---------------------------------------------------------------------------
// 2. Pts(500ms) delay.
//    Input frame seq 0 lands in output buffer with PTS 500ms (= output index
//    15 at 30fps), seq 1 in buffer 16, etc.
// ---------------------------------------------------------------------------
#[test]
fn pts_500ms_delay() {
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
    let delay = Duration::from_millis(500);
    a.queue_video_track(QueueTrackOffset::Pts(delay));
    h.start();
    a.push_video_stream(FPS_30, 10, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(1500));

    let r = h.recorded();
    // Output buffers before PTS 500ms should not contain our input.
    let frame_ns = h.frame_interval().as_nanos() as u64;
    let delay_ns = delay.as_nanos() as u64;
    let first_out_idx = (delay_ns / frame_ns) as usize;

    // The first frame of the input lands at output index `first_out_idx`,
    // because that's the first output PTS >= delay.
    let tags = r.video_tags_for_input(first_out_idx, &a.input_id);
    assert_eq!(tags, vec![(a.input_idx, 0)], "first delayed frame placement");

    // And the next 9 frames sequentially.
    for n in 1..10 {
        let tags = r.video_tags_for_input(first_out_idx + n, &a.input_id);
        assert_eq!(tags, vec![(a.input_idx, n as u32)], "frame {n} placement");
    }
}

// ---------------------------------------------------------------------------
// 3. FromStart(100ms) latches at first packet after start.
//    Push first frame post-start; second frame's offset should be the same.
// ---------------------------------------------------------------------------
#[test]
fn from_start_latches_post_start() {
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
    a.queue_video_track(QueueTrackOffset::FromStart(Duration::from_millis(100)));
    h.start();
    // Push at input-PTS 0 and 33.3ms; drop so the queue isn't waiting for a
    // newer frame to confirm older-or-equal selection on seq 1.
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));

    // FromStart(100ms): offset latches to start_pts + 100ms = 100ms (since
    // queue_start_pts ≈ 0 with our virtual clock starting at sync_point).
    // So input seq 0 lands at output PTS 100ms; seq 1 at 100ms + frame.
    // Output index for 100ms at 30fps: floor(100/33.33) = 3.
    let r = h.recorded();
    let tags_3 = r.video_tags_for_input(3, &a.input_id);
    assert_eq!(tags_3, vec![(a.input_idx, 0)], "seq 0 should land at idx 3");
    let tags_4 = r.video_tags_for_input(4, &a.input_id);
    assert_eq!(tags_4, vec![(a.input_idx, 1)], "seq 1 should land at idx 4");
}

// ---------------------------------------------------------------------------
// 4. FromStart(100ms): first packet *before* start is dropped — offset only
//    latches when the first packet arrives post-start.
// ---------------------------------------------------------------------------
#[test]
fn from_start_pre_start_packet_dropped() {
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
    a.queue_video_track(QueueTrackOffset::FromStart(Duration::from_millis(100)));

    // Push pre-start (seq=99 marker; should never appear in output).
    a.push_video_frame(99, ZERO);
    // Pre-start ticker fires as the cleanup phase processes.
    h.flush();

    h.start();
    // Now post-start frames.
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    a.drop_video();
    h.advance(Duration::from_millis(500));

    let r = h.recorded();
    // The pre-start tag (99) must not appear anywhere.
    for buf in &r.video {
        let tags = r.video_tags_for_input(
            r.video.iter().position(|b| std::ptr::eq(b, buf)).unwrap(),
            &a.input_id,
        );
        for (_, seq) in tags {
            assert_ne!(seq, 99, "pre-start frame must not surface");
        }
    }
    // Post-start frames land starting at index 3 (PTS 100ms).
    let tags_3 = r.video_tags_for_input(3, &a.input_id);
    assert_eq!(tags_3, vec![(a.input_idx, 0)], "post-start seq 0 placement");
}

// ---------------------------------------------------------------------------
// 5. None offset: latches to current queue PTS at first post-start packet.
// ---------------------------------------------------------------------------
#[test]
fn none_offset_post_start_latches_to_current_pts() {
    let h = QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(false) // pace to wall clock so queue PTS == clock
        .build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: false, // non-required: queue can advance without input
            ..Default::default()
        },
    );
    a.queue_video_track(QueueTrackOffset::None);
    h.start();
    // Advance to ~300ms post-start without pushing.
    h.advance(Duration::from_millis(300));
    // Now push first frame at input PTS 0.
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    h.advance(Duration::from_millis(500));

    let r = h.recorded();
    // Offset latches to ~300ms (give or take one frame interval); the first
    // tagged frame should appear at output PTS >= 300ms. At 30fps, idx 9
    // (PTS 300ms) is the candidate, but timing is fuzzy because the queue
    // thread runs between pulses; check the tag appears *somewhere* near 300ms.
    let mut found_idx: Option<usize> = None;
    for (i, buf) in r.video.iter().enumerate() {
        let tags = r.video_tags_for_input(i, &a.input_id);
        if !tags.is_empty() && tags[0] == (a.input_idx, 0) {
            found_idx = Some(i);
            break;
        }
        // also accept that buf may not contain our input at all (empty event)
        let _ = buf;
    }
    let found_idx = found_idx.expect("seq=0 should appear in some output buffer");
    let nominal_pts = h.nominal_video_pts(found_idx);
    assert!(
        nominal_pts >= Duration::from_millis(280)
            && nominal_pts <= Duration::from_millis(360),
        "seq=0 landed at PTS {nominal_pts:?}, expected ~300ms"
    );
}

// ---------------------------------------------------------------------------
// 6. None offset before start: latches to clock.elapsed_since(sync_point) at
//    first packet, applied via drop_old_frames_before_start. Since virtual
//    clock starts at sync_point, elapsed at first-packet time is whatever we
//    advance to before pushing.
// ---------------------------------------------------------------------------
#[test]
fn none_offset_pre_start_latches_to_elapsed() {
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
    a.queue_video_track(QueueTrackOffset::None);

    // Advance virtual clock by 200ms before pushing first packet.
    h.clock.advance(Duration::from_millis(200));
    a.push_video_frame(0, ZERO);
    // Pulse so pre-start cleanup runs and observes the packet.
    h.flush();
    a.push_video_frame(1, h.frame_interval());
    a.drop_video();

    h.start();
    h.advance(Duration::from_millis(500));

    let r = h.recorded();
    // Offset latches to 200ms. Input PTS 0 maps to queue PTS 200ms = output
    // index 6 (200/33.3 = 6).
    let mut first_idx_with_seq0 = None;
    for i in 0..r.video.len() {
        let tags = r.video_tags_for_input(i, &a.input_id);
        if tags == vec![(a.input_idx, 0)] {
            first_idx_with_seq0 = Some(i);
            break;
        }
    }
    let idx = first_idx_with_seq0.expect("seq 0 should appear");
    // Allow some slop because queue start time on virtual clock isn't exactly 200ms.
    assert!(
        (5..=8).contains(&idx),
        "seq 0 landed at idx {idx}, expected ~6"
    );
}

// ---------------------------------------------------------------------------
// 7. Offset latches once: after the first packet sets the offset, subsequent
//    packets observe the same offset.
// ---------------------------------------------------------------------------
#[test]
fn offset_latches_once() {
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
    a.queue_video_track(QueueTrackOffset::None);
    h.start();
    a.push_video_frame(0, ZERO);
    a.push_video_frame(1, h.frame_interval());
    a.push_video_frame(2, h.frame_interval() * 2);
    a.drop_video();
    h.advance(Duration::from_millis(500));

    // With None+post-start, offset latches to whatever queue_pts was at first
    // packet (~0 since we didn't advance before start). All three frames map
    // 1:1 to output buffers: seq i → idx i.
    let r = h.recorded();
    for i in 0..3 {
        let tags = r.video_tags_for_input(i, &a.input_id);
        assert_eq!(tags, vec![(a.input_idx, i as u32)], "frame {i}");
    }
}
