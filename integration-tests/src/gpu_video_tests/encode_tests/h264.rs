use crate::gpu_video_tests::harness::encode_test_runner::Codec;

/// Default high quality preset with VBR.
#[test]
fn high_quality_vbr() {
    super::high_quality_vbr(Codec::H264);
}

/// Low latency preset with CBR.
#[test]
fn low_latency_cbr() {
    super::low_latency_cbr(Codec::H264);
}

/// Output bitrate is close to the VBR average bitrate.
/// Related issues: #1745
#[test]
fn vbr_bitrate() {
    super::vbr_bitrate(Codec::H264);
}

/// Output bitrate is close to the CBR bitrate.
/// Related issues: #1745
#[test]
fn cbr_bitrate() {
    super::cbr_bitrate(Codec::H264);
}

/// Resolution not divisible by 16 (2048x858).
/// Related PRs: #1233
#[test]
fn unaligned_resolution() {
    super::unaligned_resolution(Codec::H264);
}

/// Quality doesn't degrade after the first IDR period.
/// Related issues: #1976
#[test]
fn quality_across_idr_periods() {
    super::quality_across_idr_periods(Codec::H264);
}

/// Multiple encoders and decoders working at the same time on one device.
/// Related PRs: #2249
#[test]
fn concurrent_encoders_and_decoders() {
    super::concurrent_encoders_and_decoders(Codec::H264);
}

/// Keyframes are produced when forced and reported in chunks.
/// Related PRs: #1260
#[test]
fn forced_keyframes() {
    super::forced_keyframes(Codec::H264);
}

/// Parameter sets are not inlined and can be retrieved from the encoder.
/// Related PRs: #1775
#[test]
fn out_of_band_stream_params() {
    super::out_of_band_stream_params(Codec::H264);
}

/// Color space and range are written to VUI.
/// Related PRs: #1806, #1995
#[test]
fn color_space_and_range() {
    super::color_space_and_range(Codec::H264);
}

/// Different limits of in flight submissions.
/// Related PRs: #2215, #2269
#[test]
fn max_in_flight_submissions() {
    super::max_in_flight_submissions(Codec::H264);
}

/// Encoding continues correctly after flushing.
/// Related PRs: #2215
#[test]
fn flush_mid_stream_then_continue() {
    super::flush_mid_stream_then_continue(Codec::H264);
}

/// Dropping encoders with submissions in flight.
/// Related issues: #1255, #1335. Related PRs: #2178
#[test]
fn drop_without_flush() {
    super::drop_without_flush(Codec::H264);
}

/// Dropping encoders right after the first frame.
/// Related issues: #1255, #1335. Related PRs: #2178
#[test]
fn drop_right_after_encode() {
    super::drop_right_after_encode(Codec::H264);
}

/// Dropping encoders before encoding any frame.
#[test]
fn drop_before_first_encode() {
    super::drop_before_first_encode(Codec::H264);
}

/// Frames with a wrong byte size are rejected.
#[test]
fn rejects_invalid_input() {
    super::rejects_invalid_input(Codec::H264);
}
