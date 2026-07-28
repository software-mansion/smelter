use std::{sync::Arc, time::Instant};

use smelter_render::InputId;

use crate::{
    event::EventEmitter,
    queue::{
        QueueContext, QueueInput, QueueInputOptions, QueueTrackAdvance, QueueTrackOffset,
        QueueTrackOptions,
    },
    types::Ref,
};

use super::harness::{ms, test_frame, test_samples};

fn input() -> QueueInput {
    let input_id = InputId("drain".into());
    QueueInput::new_inner(
        QueueContext::new(Instant::now(), None),
        Arc::new(EventEmitter::new()),
        &Ref::new(&input_id),
        QueueInputOptions::default(),
        None,
        None,
    )
}

fn media_track() -> QueueTrackOptions {
    QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::None,
    }
}

#[test]
fn advances_only_after_both_streams_are_drained() {
    let input = input();
    let (video, audio) = input.queue_new_track(media_track());
    assert_eq!(input.advance_track(), QueueTrackAdvance::Advanced);
    let (video, audio) = (video.unwrap(), audio.unwrap());
    video.send(test_frame(0, ms(0))).unwrap();
    audio.send(test_samples(ms(0), ms(20))).unwrap();

    let (_next_video, _next_audio) = input.queue_new_track(media_track());
    assert_eq!(
        input.advance_track(),
        QueueTrackAdvance::CurrentTrackNotDrained
    );

    drop(video);
    assert!(input.pull_video(ms(20)).unwrap().is_eos);
    assert_eq!(
        input.advance_track(),
        QueueTrackAdvance::CurrentTrackNotDrained
    );

    drop(audio);
    assert!(input.pull_audio((ms(0), ms(40))).unwrap().is_eos);
    assert_eq!(input.advance_track(), QueueTrackAdvance::Advanced);
}

#[test]
fn exact_audio_pull_has_no_mixer_lookahead() {
    let input = input();
    let (_video, audio) = input.queue_new_track(QueueTrackOptions {
        video: false,
        audio: true,
        offset: QueueTrackOffset::Pts(ms(0)),
    });
    assert_eq!(input.advance_track(), QueueTrackAdvance::Advanced);
    audio.unwrap().send(test_samples(ms(100), ms(20))).unwrap();

    assert!(
        input
            .pull_audio((ms(0), ms(50)))
            .unwrap()
            .samples
            .is_empty()
    );
    assert_eq!(
        input.pull_audio((ms(50), ms(120))).unwrap().samples.len(),
        1
    );
}
