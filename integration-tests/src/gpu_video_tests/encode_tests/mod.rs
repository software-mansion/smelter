use std::time::Duration;

use gpu_video::{
    InputFrame, RawFrameRef, VideoDeviceExt, VideoEncoderError,
    parameters::{ColorRange, ColorSpace, EncoderPreset, RateControl},
};

use crate::gpu_video_tests::{
    TestCase,
    harness::{
        encode_test_runner::{Codec, EncoderOptions, create_encoding_device},
        encoders::GvBytesEncoder,
    },
    run_concurrently,
};

mod h264;
mod h265;

const BIG_BUCK_BUNNY: &str = "h264/big_buck_bunny_10s.h264";
const ALLOWED_ERROR: f32 = 20.0;

pub(super) fn high_quality_vbr(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions::new(codec),
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn low_latency_cbr(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions {
            preset: EncoderPreset::LowLatency,
            rate_control: RateControl::ConstantBitrate {
                bitrate: 4_000_000,
                virtual_buffer_size: Duration::from_secs(1),
            },
            ..EncoderOptions::new(codec)
        },
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn vbr_bitrate(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions {
            rate_control: RateControl::VariableBitrate {
                average_bitrate: 2_000_000,
                max_bitrate: 4_000_000,
                virtual_buffer_size: Duration::from_secs(2),
            },
            bitrate_tolerance: Some(0.25),
            ..EncoderOptions::new(codec)
        },
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn cbr_bitrate(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions {
            rate_control: RateControl::ConstantBitrate {
                bitrate: 2_000_000,
                virtual_buffer_size: Duration::from_secs(1),
            },
            bitrate_tolerance: Some(0.25),
            ..EncoderOptions::new(codec)
        },
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn unaligned_resolution(codec: Codec) {
    TestCase {
        dump_file_path: "h264/frame_cropping_and_stutter.h264".into(),
        options: EncoderOptions::new(codec),
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn quality_across_idr_periods(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions {
            idr_period: 10.try_into().ok(),
            ..EncoderOptions::new(codec)
        },
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn concurrent_encoders(codec: Codec) {
    run_concurrently(&[
        &TestCase {
            dump_file_path: BIG_BUCK_BUNNY.into(),
            options: EncoderOptions::new(codec),
            allowed_error: ALLOWED_ERROR,
        },
        &TestCase {
            dump_file_path: "h264/frame_cropping_and_stutter.h264".into(),
            options: EncoderOptions::new(codec),
            allowed_error: ALLOWED_ERROR,
        },
    ]);
}

pub(super) fn forced_keyframes(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions {
            forced_keyframes: vec![1, 7, 8, 100, 249],
            ..EncoderOptions::new(codec)
        },
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn out_of_band_stream_params(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions {
            inline_stream_params: false,
            ..EncoderOptions::new(codec)
        },
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn color_space_and_range(codec: Codec) {
    for color in [
        (ColorSpace::BT709, ColorRange::Full),
        (ColorSpace::BT601Pal, ColorRange::Limited),
    ] {
        TestCase {
            dump_file_path: BIG_BUCK_BUNNY.into(),
            options: EncoderOptions {
                color: Some(color),
                max_frames: Some(30),
                ..EncoderOptions::new(codec)
            },
            allowed_error: ALLOWED_ERROR,
        }
        .run();
    }
}

pub(super) fn max_in_flight_submissions(codec: Codec) {
    for max_in_flight_submissions in [1, 8] {
        TestCase {
            dump_file_path: BIG_BUCK_BUNNY.into(),
            options: EncoderOptions {
                max_in_flight_submissions: Some(max_in_flight_submissions),
                ..EncoderOptions::new(codec)
            },
            allowed_error: ALLOWED_ERROR,
        }
        .run();
    }
}

pub(super) fn flush_mid_stream_then_continue(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions {
            flush_at: vec![0, 1, 50, 51, 120],
            ..EncoderOptions::new(codec)
        },
        allowed_error: ALLOWED_ERROR,
    }
    .run();
}

pub(super) fn drop_without_flush(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions::new(codec),
        allowed_error: ALLOWED_ERROR,
    }
    .run_drop_without_flush(30);
}

pub(super) fn drop_right_after_encode(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions::new(codec),
        allowed_error: ALLOWED_ERROR,
    }
    .run_drop_without_flush(1);
}

pub(super) fn drop_before_first_encode(codec: Codec) {
    TestCase {
        dump_file_path: BIG_BUCK_BUNNY.into(),
        options: EncoderOptions::new(codec),
        allowed_error: ALLOWED_ERROR,
    }
    .run_drop_without_flush(0);
}

pub(super) fn rejects_invalid_input(codec: Codec) {
    let (device, _queue) = create_encoding_device(codec);
    let parameters =
        EncoderOptions::new(codec).encoder_parameters(&device.video().unwrap(), 1280, 720);
    let mut encoder = GvBytesEncoder::new(&device, parameters);

    let data = vec![0; 1280 * 720];
    let result = encoder.encode_raw(
        &InputFrame {
            data: RawFrameRef {
                frame: &data,
                width: 1280,
                height: 720,
            },
            pts: None,
        },
        false,
    );
    assert!(
        matches!(
            result,
            Err(VideoEncoderError::InconsistentPictureByteSize { .. })
        ),
        "Expected InconsistentPictureByteSize, got {result:?}"
    );
}
