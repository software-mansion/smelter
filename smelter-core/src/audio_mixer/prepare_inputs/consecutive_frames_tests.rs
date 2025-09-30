use std::{sync::Arc, time::Duration};

use crate::audio_mixer::InputAudioSamples;

use super::frame_input_samples;

#[test]
fn test_continuity_between_frames() {
    // 6 samples at sample rate 48000
    let batch_duration = Duration::from_micros(125);
    let start = Duration::from_millis(20);
    let end = start + batch_duration;
    let sample_rate = 48000;
    let sample_duration = Duration::from_secs_f64(1.0 / sample_rate as f64);
    let small_error = Duration::from_secs_f64(sample_duration.as_secs_f64() * 0.005);
    let numerical_error = Duration::from_secs_f64(sample_duration.as_secs_f64() * 0.0005);
    let half_sample = Duration::from_secs_f64(sample_duration.as_secs_f64() * 0.5);

    let first_batch = Arc::new(vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)]);
    let second_batch = Arc::new(vec![(5.0, 5.0), (6.0, 6.0), (7.0, 7.0), (8.0, 8.0)]);
    let third_batch = Arc::new(vec![(9.0, 9.0), (10.0, 10.0), (11.0, 11.0), (12.0, 12.0)]);

    // shifted by half sample
    let first_batch_start = start - sample_duration - half_sample;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    let third_batch_start = first_batch_start + (8 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: first_batch.clone(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: second_batch.clone(),
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
    assert_eq!(
        frame_input_samples(
            start + batch_duration,
            end + batch_duration,
            vec![
                InputAudioSamples {
                    samples: second_batch.clone(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: third_batch.clone(),
                    start_pts: third_batch_start,
                    end_pts: third_batch_start + (4 * sample_duration)
                }
            ],
            sample_rate
        ),
        vec![
            (8.0, 8.0),
            (9.0, 9.0),
            (10.0, 10.0),
            (11.0, 11.0),
            (12.0, 12.0),
            (0.0, 0.0)
        ]
    );

    // shifted by small_error (subtract)
    let first_batch_start = start - sample_duration - small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    let third_batch_start = first_batch_start + (8 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: first_batch.clone(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: second_batch.clone(),
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
    assert_eq!(
        frame_input_samples(
            start + batch_duration,
            end + batch_duration,
            vec![
                InputAudioSamples {
                    samples: second_batch.clone(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: third_batch.clone(),
                    start_pts: third_batch_start,
                    end_pts: third_batch_start + (4 * sample_duration)
                }
            ],
            sample_rate
        ),
        vec![
            (8.0, 8.0),
            (9.0, 9.0),
            (10.0, 10.0),
            (11.0, 11.0),
            (12.0, 12.0),
            (0.0, 0.0)
        ]
    );

    // shifted by small_error (add)
    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    let third_batch_start = first_batch_start + (8 * sample_duration);
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: first_batch.clone(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: second_batch.clone(),
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
    assert_eq!(
        frame_input_samples(
            start + batch_duration,
            end + batch_duration,
            vec![
                InputAudioSamples {
                    samples: second_batch.clone(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: third_batch.clone(),
                    start_pts: third_batch_start,
                    end_pts: third_batch_start + (4 * sample_duration)
                }
            ],
            sample_rate
        ),
        vec![
            (7.0, 7.0),
            (8.0, 8.0),
            (9.0, 9.0),
            (10.0, 10.0),
            (11.0, 11.0),
            (12.0, 12.0)
        ]
    );

    // shifted by small_error (subtract) + batches overlapping between frames
    let first_batch_start = start - sample_duration - small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    let third_batch_start = first_batch_start + (8 * sample_duration) - small_error;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: first_batch.clone(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: second_batch.clone(),
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
    assert_eq!(
        frame_input_samples(
            start + batch_duration,
            end + batch_duration,
            vec![
                InputAudioSamples {
                    samples: second_batch.clone(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: third_batch.clone(),
                    start_pts: third_batch_start,
                    end_pts: third_batch_start + (4 * sample_duration)
                }
            ],
            sample_rate
        ),
        vec![
            (8.0, 8.0),
            (9.0, 9.0),
            (10.0, 10.0),
            (11.0, 11.0),
            (12.0, 12.0),
            (0.0, 0.0)
        ]
    );

    // shifted by small_error (add) + small gap between batches
    let first_batch_start = start - sample_duration + small_error;
    let second_batch_start = first_batch_start + (4 * sample_duration);
    let third_batch_start = first_batch_start + (8 * sample_duration) + small_error;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: first_batch.clone(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: second_batch.clone(),
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
    assert_eq!(
        frame_input_samples(
            start + batch_duration,
            end + batch_duration,
            vec![
                InputAudioSamples {
                    samples: second_batch.clone(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: third_batch.clone(),
                    start_pts: third_batch_start,
                    end_pts: third_batch_start + (4 * sample_duration)
                }
            ],
            sample_rate
        ),
        vec![
            (7.0, 7.0),
            (8.0, 8.0),
            (9.0, 9.0),
            (10.0, 10.0),
            (11.0, 11.0),
            (12.0, 12.0)
        ]
    );

    // Shifted only by numerical error
    // It should behave as if it was not shifted at all
    let first_batch_start = start + numerical_error;
    let second_batch_start = first_batch_start + (4 * sample_duration) - numerical_error;
    let third_batch_start = first_batch_start + (8 * sample_duration) + numerical_error;
    assert_eq!(
        frame_input_samples(
            start,
            end,
            vec![
                InputAudioSamples {
                    samples: first_batch.clone(),
                    start_pts: first_batch_start,
                    end_pts: first_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: second_batch.clone(),
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
    assert_eq!(
        frame_input_samples(
            start + batch_duration,
            end + batch_duration,
            vec![
                InputAudioSamples {
                    samples: second_batch.clone(),
                    start_pts: second_batch_start,
                    end_pts: second_batch_start + (4 * sample_duration)
                },
                InputAudioSamples {
                    samples: third_batch.clone(),
                    start_pts: third_batch_start,
                    end_pts: third_batch_start + (4 * sample_duration)
                }
            ],
            sample_rate
        ),
        vec![
            (7.0, 7.0),
            (8.0, 8.0),
            (9.0, 9.0),
            (10.0, 10.0),
            (11.0, 11.0),
            (12.0, 12.0)
        ]
    );
}
