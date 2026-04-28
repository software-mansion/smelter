//! Group 4 — EOS propagation.

use std::time::Duration;

use smelter_render::Framerate;

use crate::{
    prelude::PipelineEvent,
    queue::{
        queue_input::{QueueInputOptions, QueueTrackOffset},
        tests::harness::QueueHarness,
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
// 23. Video EOS, single input. Exactly one output buffer with EOS event;
//     `required` is true on that buffer.
// ---------------------------------------------------------------------------
#[test]
fn video_eos_single_input() {
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
    h.advance(Duration::from_millis(200));
    let r = h.recorded();

    let mut eos_count = 0;
    let mut eos_buffer_idx = None;
    for (i, buf) in r.video.iter().enumerate() {
        if let Some(PipelineEvent::EOS) = buf.frames.get(&a.input_id) {
            eos_count += 1;
            eos_buffer_idx = Some(i);
        }
    }
    assert_eq!(eos_count, 1, "exactly one EOS expected");
    let eos_idx = eos_buffer_idx.unwrap();
    assert!(
        r.video[eos_idx].required,
        "EOS-carrying buffer must be required"
    );
    // No additional `Data` event for input a after EOS.
    for buf in &r.video[(eos_idx + 1)..] {
        assert!(
            !matches!(buf.frames.get(&a.input_id), Some(PipelineEvent::Data(_))),
            "no Data events for input after EOS"
        );
    }
}

// ---------------------------------------------------------------------------
// 24. Audio EOS, single input.
// ---------------------------------------------------------------------------
#[test]
fn audio_eos_single_input() {
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
    a.push_audio_batch(0, ZERO, Duration::from_millis(20), 48000);
    a.drop_audio();
    h.advance(Duration::from_millis(200));
    let r = h.recorded();

    let mut eos_count = 0;
    for chunk in &r.audio {
        if let Some(PipelineEvent::EOS) = chunk.samples.get(&a.input_id) {
            eos_count += 1;
        }
    }
    assert_eq!(eos_count, 1, "exactly one audio EOS expected");
}

// ---------------------------------------------------------------------------
// 25. Video EOS does not stop audio chunks.
// ---------------------------------------------------------------------------
#[test]
fn video_eos_does_not_stop_audio() {
    let h = build();
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
    a.drop_video();
    a.push_audio_stream(Duration::from_millis(20), 10, ZERO);
    a.drop_audio();
    h.advance(Duration::from_millis(500));
    h.wait_for_audio_count(10);
    let r = h.recorded();
    // Should have full 10 audio chunks with our tagged data.
    let mut data_chunks = 0;
    for chunk in &r.audio {
        if matches!(chunk.samples.get(&a.input_id), Some(PipelineEvent::Data(_))) {
            data_chunks += 1;
        }
    }
    assert!(data_chunks >= 10, "audio chunks should continue after video EOS");
}

// ---------------------------------------------------------------------------
// 26. EOS on input A doesn't block input B.
// ---------------------------------------------------------------------------
#[test]
fn eos_on_a_does_not_block_b() {
    let h = build();
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
    a.drop_video();
    b.push_video_stream(FPS_30, 5, ZERO);
    b.drop_video();
    h.advance(Duration::from_millis(500));

    let r = h.recorded();
    // Input b should appear with seq 0..4.
    for i in 0..5 {
        let tags = r.video_tags_for_input(i, &b.input_id);
        assert_eq!(tags, vec![(b.input_idx, i as u32)], "input b output {i}");
    }
}

// ---------------------------------------------------------------------------
// 27. EOS during pause: pause input, drop sender while paused, resume,
//     assert EOS surfaces exactly once.
// ---------------------------------------------------------------------------
#[test]
fn eos_during_pause() {
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
    h.advance(Duration::from_millis(100));
    a.pause();
    a.drop_video();
    h.advance(Duration::from_millis(100));
    a.resume();
    h.advance(Duration::from_millis(200));

    let r = h.recorded();
    let mut eos_count = 0;
    for buf in &r.video {
        if let Some(PipelineEvent::EOS) = buf.frames.get(&a.input_id) {
            eos_count += 1;
        }
    }
    assert_eq!(eos_count, 1, "EOS should surface exactly once after resume");
}
