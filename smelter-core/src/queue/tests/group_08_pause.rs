//! Group 8 — Pause / Resume.

use std::time::Duration;

use smelter_render::Framerate;

use crate::{
    prelude::PipelineEvent,
    queue::{
        queue_input::{QueueInputOptions, QueueTrackOffset},
        tests::harness::{QueueHarness, read_audio_tag},
    },
};

const FPS_30: Framerate = Framerate { num: 30, den: 1 };
const ZERO: Duration = Duration::ZERO;

fn build() -> QueueHarness {
    QueueHarness::builder()
        .output_framerate(FPS_30)
        .ahead_of_time_processing(false) // pace to clock so pause-during-real-time works
        .build()
}

// ---------------------------------------------------------------------------
// 40. Pause holds last video frame: output buffers during pause replay the
//     pre-pause frame.
// ---------------------------------------------------------------------------
#[test]
fn pause_holds_last_video_frame() {
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
    a.push_video_stream(FPS_30, 4, ZERO);
    h.advance(Duration::from_millis(40)); // ~1 output emitted
    a.pause();
    h.advance(Duration::from_millis(200)); // pause covers ~6 outputs
    let r = h.recorded();
    // After pause, all output buffers should hold the most recent pre-pause frame.
    let last_pre_pause_idx = (40_000 / h.frame_interval().as_micros()) as u32; // 1
    let mut held_count = 0;
    for buf in r.video.iter().skip(2) {
        if let Some(PipelineEvent::Data(frame)) = buf.frames.get(&a.input_id) {
            let tag = crate::queue::tests::harness::read_video_tag(frame);
            // The held frame should match the most recent frame whose PTS was
            // <= clock-at-pause; we don't pin the exact seq because pause
            // timing depends on tick alignment, but it should not exceed
            // what was pushed before pause.
            assert!(tag.1 <= last_pre_pause_idx + 1, "held frame seq {tag:?}");
            held_count += 1;
        }
    }
    assert!(held_count >= 1, "pause should produce at least one held frame");
}

// ---------------------------------------------------------------------------
// 41. Pause produces empty audio for input.
// ---------------------------------------------------------------------------
#[test]
fn pause_produces_empty_audio() {
    let h = build();
    let a = h.add_input(
        "a",
        QueueInputOptions {
            required: true,
            ..Default::default()
        },
    );
    a.queue_audio_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_audio_stream(Duration::from_millis(20), 5, ZERO);
    h.advance(Duration::from_millis(40));
    a.pause();
    h.advance(Duration::from_millis(200));
    let r = h.recorded();
    let mut empty_data_chunks = 0;
    for chunk in &r.audio {
        if let Some(PipelineEvent::Data(batches)) = chunk.samples.get(&a.input_id)
            && batches.is_empty()
        {
            empty_data_chunks += 1;
        }
    }
    assert!(
        empty_data_chunks >= 1,
        "pause should produce at least one empty Data audio chunk for the input"
    );
}

// ---------------------------------------------------------------------------
// 42. Resume shifts offset by paused duration. Frames pushed post-resume land
//     at output PTS = original + paused duration.
// ---------------------------------------------------------------------------
#[test]
fn resume_shifts_offset() {
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
    a.pause();
    h.advance(Duration::from_millis(200));
    a.resume();
    a.push_video_frame(2, h.frame_interval() * 2);
    a.push_video_frame(3, h.frame_interval() * 3);
    a.drop_video();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();
    // Frames seq 2 and 3 should land later than they would have without pause.
    let mut found_2 = None;
    for (i, buf) in r.video.iter().enumerate() {
        if let Some(PipelineEvent::Data(frame)) = buf.frames.get(&a.input_id)
            && crate::queue::tests::harness::read_video_tag(frame) == (a.input_idx, 2)
        {
            found_2 = Some(i);
            break;
        }
    }
    let idx = found_2.expect("seq 2 should appear");
    // Without pause, seq 2 would land at output index 2 (PTS 66ms). With
    // 200ms pause, it should land at output index >= 2 + 6 = 8.
    assert!(
        idx >= 8,
        "seq 2 should be shifted by paused duration, got idx {idx}"
    );
}

// ---------------------------------------------------------------------------
// 43. Pause before start, resume before start: behaves as if no pause.
// ---------------------------------------------------------------------------
#[test]
fn pause_resume_pre_start_no_effect() {
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
    a.pause();
    a.resume();
    h.start();
    a.push_video_stream(FPS_30, 5, ZERO);
    a.drop_video();
    h.advance(Duration::from_millis(300));
    let r = h.recorded();
    // First five outputs should pair with seq 0..4 (no shift).
    for i in 0..5 {
        let tags = r.video_tags_for_input(i, &a.input_id);
        assert_eq!(tags, vec![(a.input_idx, i as u32)]);
    }
}

// ---------------------------------------------------------------------------
// 44. Double pause / double resume idempotent.
// ---------------------------------------------------------------------------
#[test]
fn pause_resume_idempotent() {
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
    a.pause();
    a.pause(); // no-op
    h.advance(Duration::from_millis(100));
    a.resume();
    a.resume(); // no-op
    a.push_video_frame(2, h.frame_interval() * 2);
    a.drop_video();
    h.advance(Duration::from_millis(300));
    // No panic, frames flow.
    let r = h.recorded();
    assert!(
        r.video.iter().any(|b| {
            matches!(b.frames.get(&a.input_id), Some(PipelineEvent::Data(_)))
        }),
        "video should still flow after redundant pause/resume"
    );
}

// ---------------------------------------------------------------------------
// 45. AV pair stays aligned across pause: video tag (in0,N) and audio batch
//     (in0,N) should still co-occur in similar output PTS regions.
// ---------------------------------------------------------------------------
#[test]
fn av_aligned_across_pause() {
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
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_frame(0, ZERO);
    a.push_audio_batch(0, ZERO, Duration::from_millis(20), 48000);
    h.advance(Duration::from_millis(40));
    a.pause();
    h.advance(Duration::from_millis(200));
    a.resume();
    a.push_video_frame(1, h.frame_interval());
    a.push_video_frame(2, h.frame_interval() * 2);
    a.push_audio_batch(1, Duration::from_millis(20), Duration::from_millis(20), 48000);
    a.push_audio_batch(2, Duration::from_millis(40), Duration::from_millis(20), 48000);
    a.drop_video();
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    let r = h.recorded();

    // Find output buffer indexes where video seq 1 and audio batch seq 1 land.
    let mut video_1_idx = None;
    for (i, buf) in r.video.iter().enumerate() {
        if let Some(PipelineEvent::Data(frame)) = buf.frames.get(&a.input_id)
            && crate::queue::tests::harness::read_video_tag(frame) == (a.input_idx, 1)
        {
            video_1_idx = Some(i);
            break;
        }
    }
    let mut audio_1_idx = None;
    for (i, chunk) in r.audio.iter().enumerate() {
        if let Some(PipelineEvent::Data(batches)) = chunk.samples.get(&a.input_id) {
            for b in batches {
                if read_audio_tag(b) == (a.input_idx, 1) {
                    audio_1_idx = Some(i);
                }
            }
        }
    }
    let v = video_1_idx.expect("video seq 1 should land somewhere");
    let aa = audio_1_idx.expect("audio batch 1 should land somewhere");
    // Audio chunks are 20ms; video frames are 33.3ms. PTS region:
    let video_pts = h.nominal_video_pts(v).as_millis() as i64;
    let audio_pts = (aa as u64 * 20) as i64;
    assert!(
        (video_pts - audio_pts).abs() <= 50,
        "AV should be within ~50ms after pause/resume; v_pts={video_pts}ms a_pts={audio_pts}ms"
    );
}
