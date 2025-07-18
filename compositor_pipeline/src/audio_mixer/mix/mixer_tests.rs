use crate::audio_mixer::mix::*;

const VOL_DOWN_THRESHOLD: f64 = 25_000.0;
const VOL_UP_THRESHOLD: f64 = 20_000.0;
const VOL_DOWN_INCREMENT: f64 = 0.01;
const VOL_UP_INCREMENT: f64 = 0.005;

#[test]
fn sum_scaler_no_scaling_test() {
    let mut mixer = SampleMixer::new(
        VOL_DOWN_THRESHOLD,
        VOL_UP_THRESHOLD,
        VOL_DOWN_INCREMENT,
        VOL_UP_INCREMENT,
    );

    let input_samples: Vec<(i64, i64)> = vec![
        (10, -10),
        (-20, 30),
        (1000, 1000),
        (15_000, 15_000),
        (-20_000, -20_000),
    ];

    let actual_samples = mixer.scale_samples(input_samples);

    assert_eq!(mixer.scaling_factor, 1.0);
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
    let mut mixer = SampleMixer::new(
        VOL_DOWN_THRESHOLD,
        VOL_UP_THRESHOLD,
        VOL_DOWN_INCREMENT,
        VOL_UP_INCREMENT,
    );

    let input_samples: Vec<(i64, i64)> = vec![
        (30_000, -30_000),
        (34_000, -34_000), // out of i16 range
        (27_000, -27_000),
        (31_987, -31_987),
        (21_111, -21_111),
    ];

    let actual_samples = mixer.scale_samples(input_samples);

    assert_eq!(mixer.scaling_factor, 0.99);
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
    let mut mixer = SampleMixer::new(
        VOL_DOWN_THRESHOLD,
        VOL_UP_THRESHOLD,
        VOL_DOWN_INCREMENT,
        VOL_UP_INCREMENT,
    );

    // This chunk triggers volume decrease
    let input_chunk_1: Vec<(i64, i64)> = vec![
        (30_000, -30_000),
        (34_000, -34_000), // out of i16 range
        (27_000, -27_000),
        (31_987, -31_987),
        (21_111, -21_111),
    ];

    // This chunk doesn't change volume
    let input_chunk_2: Vec<(i64, i64)> = vec![
        (5000, -5000),
        (1111, 1111),
        (-12_000, 12_000),
        (21_000, 22_000),
        (11_000, 9999),
    ];

    // This chunk causes volume to increase
    let input_chunk_3: Vec<(i64, i64)> = vec![
        (15_000, 15_000),
        (11_000, 11_000),
        (10_000, 10_000),
        (-1000, -1000),
        (-15_000, -15_000),
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
            (30_000, -30_000),    // 1.0
            (i16::MAX, i16::MIN), // 0.998 (out of range)
            (26_892, -26_892),    // 0.996
            (31_795, -31_795),    // 0.994
            (20_942, -20_942),    // 0.992
        ]
    );

    assert_eq!(actual_scaling_factor_2, 0.99);
    assert_eq!(
        actual_chunk_2,
        vec![
            (4950, -4950),
            (1100, 1100),
            (-11_880, 11_880),
            (20_790, 21_780),
            (10_890, 9899),
        ]
    );

    assert_eq!(actual_scaling_factor_3, 0.995);
    assert_eq!(
        actual_chunk_3,
        vec![
            (14_850, 14_850),   // 0.99
            (10_901, 10_901),   // 0.991
            (9920, 9920),       // 0.992
            (-993, -993),       // 0.993
            (-14_910, -14_910), // 0.994
        ]
    );
}
