use crate::audio_mixer::mix::*;
use tracing_subscriber::{self, EnvFilter};

const SCALING_THRESHOLD: f64 = 25_000.0f64;
const SCALING_INCREMENT: f64 = 0.01f64;

fn set_testing_subscriber() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

#[test]
fn sum_scaler_no_scaling_test() {
    let mut mixer = SampleMixer::new(SCALING_THRESHOLD, SCALING_INCREMENT);

    let input_samples: Vec<(i64, i64)> = vec![
        (10, -10),
        (-20, 30),
        (1000, 1000),
        (15_000, 15_000),
        (-20_000, -20_000),
    ];

    let actual_samples = mixer.sum_scale(input_samples);

    assert_eq!(mixer.scaling_factor, 1.0f64);
    assert_eq!(
        actual_samples,
        vec![
            (10, -10),
            (-20, 30),
            (1000, 1000),
            (15_000, 15_000),
            (-20_000, -20_000),
        ]
    );
}

#[test]
fn sum_scaler_basic_scaling_test() {
    let mut mixer = SampleMixer::new(SCALING_THRESHOLD, SCALING_INCREMENT);

    let input_samples: Vec<(i64, i64)> = vec![
        (30_000, -30_000),
        (34_000, -34_000), // out of i16 range
        (27_000, -27_000),
        (31_987, -31_987),
        (21_111, -21_111),
    ];

    let actual_samples = mixer.sum_scale(input_samples);

    assert_eq!(mixer.scaling_factor, 0.99f64);
    assert_eq!(
        actual_samples,
        // Numbers are values of scaling factor for given sample
        vec![
            (30_000, -30_000),    // 1.0
            (i16::MAX, i16::MIN), // 0.998 (out of range)
            (26_892, -26_892),    // 0.996
            (31_795, -31_795),    // 0.994
            (20_942, -20_942),    // 0.992
        ]
    );
}

#[test]
fn sum_scaler_decrease_and_increase_volume_test() {
    set_testing_subscriber();

    let mut mixer = SampleMixer::new(SCALING_THRESHOLD, SCALING_INCREMENT);

    let input_chunk_1: Vec<(i64, i64)> = vec![
        (30_000, -30_000),
        (34_000, -34_000), // out of i16 range
        (27_000, -27_000),
        (31_987, -31_987),
        (21_111, -21_111),
    ];

    let input_chunk_2: Vec<(i64, i64)> = vec![
        (5000, -5000),
        (1111, 1111),
        (-12_000, 12_000),
        (21_000, 22_000),
        (11_000, 9999),
    ];

    let actual_chunk_1 = mixer.sum_scale(input_chunk_1);
    let actual_scaling_factor_1 = mixer.scaling_factor;

    let actual_chunk_2 = mixer.sum_scale(input_chunk_2);
    let actual_scaling_factor_2 = mixer.scaling_factor;

    assert_eq!(actual_scaling_factor_1, 0.99f64);
    assert_eq!(
        actual_chunk_1,
        vec![
            (30_000, -30_000),    // 1.0
            (i16::MAX, i16::MIN), // 0.998 (out of range)
            (26_892, -26_892),    // 0.996
            (31_795, -31_795),    // 0.994
            (20_942, -20_942),    // 0.992
        ]
    );

    assert_eq!(actual_scaling_factor_2, 1.0f64);
    assert_eq!(
        actual_chunk_2,
        vec![
            (4950, -4950),     // 0.99
            (1102, 1102),      // 0.992
            (-11_928, 11_928), // 0.994
            (20_916, 21_912),  // 0.996
            (10_978, 9979),    // 0.998
        ]
    );
}
