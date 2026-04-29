//! Group 14 — Smoke / end-to-end.

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

// ---------------------------------------------------------------------------
// 100. AV 1s simulation, 30fps video, 20ms audio, 2 inputs.
// ---------------------------------------------------------------------------
#[test]
fn av_1s_two_inputs() {
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
    a.queue_av_track(QueueTrackOffset::Pts(ZERO));
    b.queue_av_track(QueueTrackOffset::Pts(ZERO));
    h.start();
    a.push_video_stream(FPS_30, 30, ZERO);
    a.push_audio_stream(Duration::from_millis(20), 50, ZERO);
    a.drop_video();
    a.drop_audio();
    b.push_video_stream(FPS_30, 30, ZERO);
    b.push_audio_stream(Duration::from_millis(20), 50, ZERO);
    b.drop_video();
    b.drop_audio();
    h.advance(Duration::from_millis(1500));
    h.wait_for_video_count(30);
    h.wait_for_audio_count(50);
    let r = h.recorded();
    assert!(r.video.len() >= 30);
    assert!(r.audio.len() >= 50);

    // Every video output buffer should have data (or EOS) for both inputs.
    for buf in r.video.iter().take(30) {
        assert!(matches!(
            buf.frames.get(&a.input_id),
            Some(PipelineEvent::Data(_)) | Some(PipelineEvent::EOS)
        ));
        assert!(matches!(
            buf.frames.get(&b.input_id),
            Some(PipelineEvent::Data(_)) | Some(PipelineEvent::EOS)
        ));
    }
}
