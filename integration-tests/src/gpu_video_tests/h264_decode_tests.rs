use gpu_video::parameters::DecoderParameters;

use crate::gpu_video_tests::{TestCase, harness::decode_test_runner::DecoderOptions};

/// Just 10s of big buck bunny.
#[test]
fn big_buck_bunny() {
    TestCase {
        dump_file_path: "h264/big_buck_bunny_10s.h264".into(),
        options: DecoderOptions::H264(DecoderParameters::default()),
        allowed_error: 0.0,
    }
    .run();
}

/// Resolution changes: 360p -> 720p -> 1080p -> 720p -> 360p.
/// Related PRs: #1787, #2080
#[test]
fn changing_resolution() {
    TestCase {
        dump_file_path: "h264/changing_resolution_25fps.h264".into(),
        options: DecoderOptions::H264(DecoderParameters::default()),
        allowed_error: 0.0,
    }
    .run();
}

/// H.264 profile changes: baseline -> main -> high -> main -> baseline.
#[test]
fn changing_profile() {
    TestCase {
        dump_file_path: "h264/changing_profile_25fps.h264".into(),
        options: DecoderOptions::H264(DecoderParameters::default()),
        allowed_error: 0.0,
    }
    .run();
}

/// Tests for problems with frame cropping and stutter at frame 133.
/// Related PRs: #2071
#[test]
fn frame_cropping() {
    TestCase {
        dump_file_path: "h264/frame_cropping_and_stutter.h264".into(),
        options: DecoderOptions::H264(DecoderParameters::default()),
        allowed_error: 0.0,
    }
    .run();
}

/// Tests regression where a short reference was incorrectly deleted.
/// Related PRs: #1991
#[test]
fn short_reference_deletion() {
    TestCase {
        dump_file_path: "h264/short_reference_deletion.h264".into(),
        options: DecoderOptions::H264(DecoderParameters::default()),
        allowed_error: 0.0,
    }
    .run();
}
