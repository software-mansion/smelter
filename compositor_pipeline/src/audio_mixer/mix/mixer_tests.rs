use tracing_subscriber::EnvFilter;

use crate::audio_mixer::mix::*;

const VOL_DOWN_THRESHOLD: f64 = 0.8;
const VOL_UP_THRESHOLD: f64 = 0.5;
const VOL_DOWN_INCREMENT: f64 = 0.01;
const VOL_UP_INCREMENT: f64 = 0.005;

#[test]
fn sum_scaler_no_scaling_test() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let mut mixer = SampleMixer::new(
        VOL_DOWN_THRESHOLD,
        VOL_UP_THRESHOLD,
        VOL_DOWN_INCREMENT,
        VOL_UP_INCREMENT,
    );

    let input_samples: Vec<(f64, f64)> = vec![
        (0.01, -0.010),
        (-0.02, 0.03),
        (0.1, 0.1),
        (0.4, 0.4),
        (-0.6, 0.5),
    ];

    let actual_samples = mixer.scale_samples(input_samples);

    assert_eq!(mixer.scaling_factor, 1.0);
    assert_eq!(
        actual_samples,
        vec![
            (0.01, -0.010),
            (-0.02, 0.03),
            (0.1, 0.1),
            (0.4, 0.4),
            (-0.6, 0.5),
        ]
    );
}

#[test]
fn sum_scaler_basic_scaling_test() {
    let mut mixer = SampleMixer::new(
        VOL_DOWN_THRESHOLD,
        VOL_UP_THRESHOLD,
        VOL_DOWN_INCREMENT,
        VOL_UP_INCREMENT,
    );

    let input_samples: Vec<(f64, f64)> = vec![
        (0.9, -0.9),
        (1.1, -1.1), // out of range
        (0.95, -0.95),
        (0.98, -0.98),
        (0.7, -0.7),
    ];

    let actual_samples = mixer.scale_samples(input_samples);

    assert_eq!(mixer.scaling_factor, 0.99);
    assert_eq!(
        actual_samples,
        // Numbers are values of scaling factor for given sample
        vec![
            (1.0 * 0.9, 1.0 * -0.9),
            (1.0, -1.0), // out of range
            (0.996 * 0.95, 0.996 * -0.95),
            (0.994 * 0.98, 0.994 * -0.98),
            (0.992 * 0.7, 0.992 * -0.7),
        ]
    );
}

#[test]
fn sum_scaler_decrease_and_increase_volume_test() {
    let mut mixer = SampleMixer::new(
        VOL_DOWN_THRESHOLD,
        VOL_UP_THRESHOLD,
        VOL_DOWN_INCREMENT,
        VOL_UP_INCREMENT,
    );

    // This chunk triggers volume decrease
    let input_chunk_1: Vec<(f64, f64)> = vec![
        (0.9, -0.9),
        (1.1, -1.1), // out of i16 range
        (0.85, -0.85),
        (0.95, -0.95),
        (0.7, -0.7),
    ];

    // This chunk doesn't change volume
    let input_chunk_2: Vec<(f64, f64)> = vec![
        (0.3, -0.3),
        (0.2, 0.2),
        (-0.4, 0.4),
        (0.7, 0.7),
        (0.35, 0.35),
    ];

    // This chunk causes volume to increase
    let input_chunk_3: Vec<(f64, f64)> = vec![
        (0.4, 0.4),
        (0.3, 0.4),
        (0.25, 0.25),
        (-0.35, -0.35),
        (-0.45, -0.45),
    ];

    let actual_chunk_1 = mixer.scale_samples(input_chunk_1);
    let actual_scaling_factor_1 = mixer.scaling_factor;

    let actual_chunk_2 = mixer.scale_samples(input_chunk_2);
    let actual_scaling_factor_2 = mixer.scaling_factor;

    let actual_chunk_3 = mixer.scale_samples(input_chunk_3);
    let actual_scaling_factor_3 = mixer.scaling_factor;

    assert_eq!(actual_scaling_factor_1, 0.99);
    assert_eq!(
        actual_chunk_1,
        vec![
            (1.0 * 0.9, 1.0 * -0.9),
            (1.0, -1.0), // out of (-1, 1) range
            (0.996 * 0.85, 0.996 * -0.85),
            (0.994 * 0.95, 0.994 * -0.95),
            (0.992 * 0.7, 0.992 * -0.7),
        ]
    );

    assert_eq!(actual_scaling_factor_2, 0.99);
    assert_eq!(
        actual_chunk_2,
        vec![
            (0.99 * 0.3, 0.99 * -0.3),
            (0.99 * 0.2, 0.99 * 0.2),
            (0.99 * -0.4, 0.99 * 0.4),
            (0.99 * 0.7, 0.99 * 0.7),
            (0.99 * 0.35, 0.99 * 0.35),
        ]
    );

    assert_eq!(actual_scaling_factor_3, 0.995);
    assert_eq!(
        actual_chunk_3,
        vec![
            (0.99 * 0.4, 0.99 * 0.4),
            (0.991 * 0.3, 0.991 * 0.4),
            (0.992 * 0.25, 0.992 * 0.25),
            (0.993 * -0.35, 0.993 * -0.35),
            (0.994 * -0.45, 0.994 * -0.45),
        ]
    );
}
