use std::time::Duration;

use crate::prelude::*;

use super::frame_input_samples;

#[test]
fn test_prepare_inputs() {
    // 6 samples at sample rate 48000
    let batch_duration = Duration::from_micros(125);
    let start = Duration::from_millis(20);
    let end = start + batch_duration;
    let sample_rate = 48000;
    let sample_duration = Duration::from_secs_f64(1.0 / sample_rate as f64);
    let small_error = Duration::from_secs_f64(sample_duration.as_secs_f64() * 0.001);
    let half_sample = Duration::from_secs_f64(sample_duration.as_secs_f64() * 0.5);

    assert_eq!(
        frame_input_samples(start, end, vec![], sample_rate),
        vec![
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0)
        ]
    );

    let first_batch_start = start - small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0),
            (6.0, 6.0)
        ]
    );

    let first_batch_start = start - half_sample;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0),
            (6.0, 6.0)
        ]
    );

    let first_batch_start = start + small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (0.0, 0.0),
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0)
        ]
    );

    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0),
            (6.0, 6.0)
        ]
    );

    let first_batch_start = start - sample_duration - small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0),
            (6.0, 6.0),
            (7.0, 7.0)
        ]
    );

    //slightly overlapping batches
    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration) - small_error;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0),
            (6.0, 6.0)
        ]
    );

    // batches with small gap (small error)
    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration) + small_error;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0),
            (6.0, 6.0)
        ]
    );

    //slightly overlapping batches (more than half sample)
    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration) - small_error - half_sample;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0),
            (6.0, 6.0)
        ]
    );

    // batches with small gap (more than half sample)
    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration) + small_error + half_sample;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (5.0, 5.0),
            (6.0, 6.0)
        ]
    );

    //slightly overlapping batches (more than a sample)
    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start =
        first_batch_start + (4 * sample_duration) - small_error - sample_duration;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (6.0, 6.0),
            (7.0, 7.0)
        ]
    );

    // batches with small gap (more than half sample)
    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start =
        first_batch_start + (4 * sample_duration) + small_error + sample_duration;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (1.0, 1.0),
            (2.0, 2.0),
            (3.0, 3.0),
            (4.0, 4.0),
            (0.0, 0.0),
            (5.0, 5.0)
        ]
    );

    // Severly missaligned timestamps (to the left)
    let first_batch_start = start - batch_duration;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)].into(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)].into(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
            ],
            sample_rate
        ),
        vec![
            (7.0, 7.0),
            (8.0, 8.0),
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0),
        ],
    );
}
