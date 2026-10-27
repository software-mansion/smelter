use crate::gpu_video_tests::harness::encode_test_runner::Codec;

/// Default high quality preset with VBR.
#[test]
fn high_quality_vbr() {
    super::high_quality_vbr(Codec::H265);
}

/// Low latency preset with CBR.
#[test]
fn low_latency_cbr() {
    super::low_latency_cbr(Codec::H265);
}

/// Output bitrate is close to the VBR average bitrate.
#[test]
fn vbr_bitrate() {
    super::vbr_bitrate(Codec::H265);
}

/// Output bitrate is close to the CBR bitrate.
#[test]
fn cbr_bitrate() {
    super::cbr_bitrate(Codec::H265);
}

/// Resolution not divisible by 16 (2048x858).
/// Related issues: #1210
#[test]
fn unaligned_resolution() {
    super::unaligned_resolution(Codec::H265);
}

/// Quality doesn't degrade after the first IDR period.
/// Related issues: #1976
#[test]
fn quality_across_idr_periods() {
    super::quality_across_idr_periods(Codec::H265);
}

/// Keyframes are produced when forced and reported in chunks.
#[test]
fn forced_keyframes() {
    super::forced_keyframes(Codec::H265);
}

/// Parameter sets are not inlined and can be retrieved from the encoder.
#[test]
fn out_of_band_stream_params() {
    super::out_of_band_stream_params(Codec::H265);
}

/// Color space and range are written to VUI.
#[test]
fn color_space_and_range() {
    super::color_space_and_range(Codec::H265);
}

/// Different limits of in flight submissions.
/// Related PRs: #2269
#[test]
fn max_in_flight_submissions() {
    super::max_in_flight_submissions(Codec::H265);
}

/// Encoding continues correctly after flushing.
#[test]
fn flush_mid_stream_then_continue() {
    super::flush_mid_stream_then_continue(Codec::H265);
}

/// Dropping encoders with submissions in flight.
/// Related issues: #1255, #1335. Related PRs: #2178
#[test]
fn drop_without_flush() {
    super::drop_without_flush(Codec::H265);
}

/// Dropping encoders right after the first frame.
/// Related issues: #1255, #1335. Related PRs: #2178
#[test]
fn drop_right_after_encode() {
    super::drop_right_after_encode(Codec::H265);
}

/// Dropping encoders before encoding any frame.
/// Related PRs: #2294
#[test]
fn drop_before_first_encode() {
    super::drop_before_first_encode(Codec::H265);
}

/// Frames with a wrong byte size are rejected.
#[test]
fn rejects_invalid_input() {
    super::rejects_invalid_input(Codec::H265);
}
