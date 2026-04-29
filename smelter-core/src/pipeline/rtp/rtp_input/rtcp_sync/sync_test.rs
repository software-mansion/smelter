use std::{
    thread,
    time::{Duration, Instant},
};

use crate::pipeline::rtp::rtp_input::rtcp_sync::{RtpNtpSyncPoint, RtpTimestampSync};

const POW_2_32: u64 = 1u64 << 32;

// Allowed error that can be cause by speed of the runtime execution e.g. sleep precision
const PREC_RUNTIME: Duration = Duration::from_millis(5);

// Represents 09/09/2025, 1:00:21 PM
const REFERENCE_NTP_TIME: u64 = 3966409461 * POW_2_32;

#[test]
fn test_rtcp_sync_pts_from_zero() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000);

    let stream_1_first_pts = stream_1.pts_from_timestamp(0);
    thread::sleep(Duration::from_millis(100));
    let stream_2_first_pts = stream_2.pts_from_timestamp(0);
    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::from_millis(100), PREC_RUNTIME);

    let stream_1_second_pts = stream_1.pts_from_timestamp(1_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(1_000);

    assert_eq!(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(1)
    );
    assert_eq!(
        stream_2_second_pts,
        stream_2_first_pts + Duration::from_secs(1)
    );

    // provide sync information
    assert!(sync_point.ntp_time.read().unwrap().is_none());
    stream_1.on_sender_report(REFERENCE_NTP_TIME, 0);
    let sync_point_ntp_time = sync_point.ntp_time.read().unwrap().unwrap();

    // stream_2 offset is one second relative to stream_1
    stream_2.on_sender_report(REFERENCE_NTP_TIME, 0);

    // check is sync point did not change
    assert_eq!(
        sync_point.ntp_time.read().unwrap().unwrap(),
        sync_point_ntp_time
    );

    // SR sets the target offset; per-packet slew converges sync_offset toward
    // it. Drain so we observe the converged state.
    drain_slew(&mut stream_1, 1_000);
    drain_slew(&mut stream_2, 1_000);

    let stream_1_second_pts_old = stream_1_second_pts;
    let stream_2_second_pts_old = stream_2_second_pts;
    let stream_1_second_pts = stream_1.pts_from_timestamp(1_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(1_000);
    // check if stream_1 pts did not change
    assert_eq!(stream_1_second_pts, stream_1_second_pts_old);
    // check if stream_2 is the same as stream 1 for the same pts (no 100ms difference anymore)
    assert_eq!(stream_2_second_pts, stream_1_second_pts);
    // check if pts shifted by about 100ms (sleep time between initial packets)
    assert_duration_eq(
        stream_2_second_pts + Duration::from_millis(100),
        stream_2_second_pts_old,
        PREC_RUNTIME,
    );
}

#[test]
fn test_rtcp_sync_pts_from_non_zero() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000);

    let stream_1_first_pts = stream_1.pts_from_timestamp(60_000);
    thread::sleep(Duration::from_millis(100));
    let stream_2_first_pts = stream_2.pts_from_timestamp(90_000);

    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::from_millis(100), PREC_RUNTIME);

    let stream_1_second_pts = stream_1.pts_from_timestamp(61_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(91_000);

    assert_eq!(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(1)
    );
    assert_eq!(
        stream_2_second_pts,
        stream_2_first_pts + Duration::from_secs(1)
    );

    // provide sync information
    assert!(sync_point.ntp_time.read().unwrap().is_none());

    stream_1.on_sender_report(REFERENCE_NTP_TIME, 0);

    let sync_point_ntp_time = sync_point.ntp_time.read().unwrap().unwrap();

    // stream_2 offset is one second relative to stream starts, but offset
    // between PTS values representing the same time is 29 second.
    stream_2.on_sender_report(REFERENCE_NTP_TIME, 30_000);

    // check is sync point did not change
    assert_eq!(
        sync_point.ntp_time.read().unwrap().unwrap(),
        sync_point_ntp_time
    );

    drain_slew(&mut stream_1, 61_000);
    drain_slew(&mut stream_2, 91_000);

    let stream_1_second_pts_old = stream_1_second_pts;
    let stream_2_second_pts_old = stream_2_second_pts;
    let stream_1_second_pts = stream_1.pts_from_timestamp(61_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(91_000);
    // check if stream_1 pts did not change
    assert_eq!(stream_1_second_pts, stream_1_second_pts_old);
    // check if stream_2 is the same as stream 1 for the same pts (no 100ms difference anymore)
    assert_eq!(stream_2_second_pts, stream_1_second_pts);
    // check if pts shifted by about 100ms (sleep time between initial packets)
    assert_duration_eq(
        stream_2_second_pts + Duration::from_millis(100),
        stream_2_second_pts_old,
        PREC_RUNTIME,
    );
}

#[test]
fn test_rtcp_sync_pts_from_non_zero_different_clocks() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 3_000);

    let stream_1_first_pts = stream_1.pts_from_timestamp(60_000);
    thread::sleep(Duration::from_millis(100));
    let stream_2_first_pts = stream_2.pts_from_timestamp(90_000 * 3);

    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::from_millis(100), PREC_RUNTIME);

    let stream_1_second_pts = stream_1.pts_from_timestamp(61_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(91_000 * 3);

    assert_eq!(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(1)
    );
    assert_eq!(
        stream_2_second_pts,
        stream_2_first_pts + Duration::from_secs(1)
    );

    // provide sync information
    assert!(sync_point.ntp_time.read().unwrap().is_none());

    stream_1.on_sender_report(REFERENCE_NTP_TIME, 0);

    let sync_point_ntp_time = sync_point.ntp_time.read().unwrap().unwrap();

    // stream_2 offset is one second relative to stream starts, but offset
    // between PTS values representing the same time is 29 second.
    stream_2.on_sender_report(REFERENCE_NTP_TIME, 30_000 * 3);

    // check is sync point did not change
    assert_eq!(
        sync_point.ntp_time.read().unwrap().unwrap(),
        sync_point_ntp_time
    );

    drain_slew(&mut stream_1, 61_000);
    drain_slew(&mut stream_2, 91_000 * 3);

    let stream_1_second_pts_old = stream_1_second_pts;
    let stream_2_second_pts_old = stream_2_second_pts;
    let stream_1_second_pts = stream_1.pts_from_timestamp(61_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(91_000 * 3);
    // check if stream_1 pts did not change
    assert_eq!(stream_1_second_pts, stream_1_second_pts_old);
    // check if stream_2 is the same as stream 1 for the same pts (no 100ms difference anymore)
    assert_eq!(stream_2_second_pts, stream_1_second_pts);
    // check if pts shifted by about 100ms (sleep time between initial packets)
    assert_duration_eq(
        stream_2_second_pts + Duration::from_millis(100),
        stream_2_second_pts_old,
        PREC_RUNTIME,
    );
}

#[test]
fn test_rtcp_sync_pts_with_rollover_before_sender_report_first_stream() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000);

    let stream_1_first_rtp_timestamp = u32::MAX - 5_000 + 1;
    let stream_2_first_rtp_timestamp = 100_000;

    let stream_1_first_pts = stream_1.pts_from_timestamp(stream_1_first_rtp_timestamp);
    thread::sleep(Duration::from_millis(100));
    let stream_2_first_pts = stream_2.pts_from_timestamp(stream_2_first_rtp_timestamp);

    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::from_millis(100), PREC_RUNTIME);

    // after 10 seconds
    let stream_1_second_pts = stream_1.pts_from_timestamp(5_000); // rolled over
    let stream_2_second_pts = stream_2.pts_from_timestamp(stream_2_first_rtp_timestamp + 10_000);

    assert_eq!(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(10)
    );
    assert_eq!(
        stream_2_second_pts,
        stream_2_first_pts + Duration::from_secs(10)
    );

    // provide sync information
    assert!(sync_point.ntp_time.read().unwrap().is_none());

    // could be any number larger than 60_000_000, otherwise we would return timestamp from
    // before sync point
    stream_1.on_sender_report(REFERENCE_NTP_TIME, stream_1_first_rtp_timestamp);

    let sync_point_ntp_time = sync_point.ntp_time.read().unwrap().unwrap();

    // stream_2 offset is one second relative to stream starts, but offset
    // between PTS values representing the same time is 29 second.
    stream_2.on_sender_report(REFERENCE_NTP_TIME, stream_2_first_rtp_timestamp);

    // check is sync point did not change
    assert_eq!(
        sync_point.ntp_time.read().unwrap().unwrap(),
        sync_point_ntp_time
    );

    drain_slew(&mut stream_1, 5_000);
    drain_slew(&mut stream_2, stream_2_first_rtp_timestamp + 10_000);

    let stream_1_second_pts_old = stream_1_second_pts;
    let stream_2_second_pts_old = stream_2_second_pts;
    let stream_1_second_pts = stream_1.pts_from_timestamp(5_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(stream_2_first_rtp_timestamp + 10_000);
    // check if stream_1 pts did not change
    assert_eq!(stream_1_second_pts, stream_1_second_pts_old);
    // check if stream_2 is the same as stream 1 for the same pts (no 100ms difference anymore)
    assert_eq!(stream_2_second_pts, stream_1_second_pts);
    // check if pts shifted by about 100ms (sleep time between initial packets)
    assert_duration_eq(
        stream_2_second_pts + Duration::from_millis(100),
        stream_2_second_pts_old,
        PREC_RUNTIME,
    );
}

#[test]
fn test_rtcp_sync_pts_with_rollover_before_sender_report_second_stream() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000);

    let stream_1_first_rtp_timestamp = 100_000;
    let stream_2_first_rtp_timestamp = u32::MAX - 5_000 + 1;

    let stream_1_first_pts = stream_1.pts_from_timestamp(stream_1_first_rtp_timestamp);
    thread::sleep(Duration::from_millis(100));
    let stream_2_first_pts = stream_2.pts_from_timestamp(stream_2_first_rtp_timestamp);

    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::from_millis(100), PREC_RUNTIME);

    // after 10 seconds
    let stream_1_second_pts = stream_1.pts_from_timestamp(stream_1_first_rtp_timestamp + 10_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(5_000);

    assert_eq!(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(10)
    );
    assert_eq!(
        stream_2_second_pts,
        stream_2_first_pts + Duration::from_secs(10)
    );

    // provide sync information
    assert!(sync_point.ntp_time.read().unwrap().is_none());

    // could be any number larger than 60_000_000, otherwise we would return timestamp from
    // before sync point
    stream_1.on_sender_report(REFERENCE_NTP_TIME, stream_1_first_rtp_timestamp);

    let sync_point_ntp_time = sync_point.ntp_time.read().unwrap().unwrap();

    // stream_2 offset is one second relative to stream starts, but offset
    // between PTS values representing the same time is 29 second.
    stream_2.on_sender_report(REFERENCE_NTP_TIME, stream_2_first_rtp_timestamp);

    // check is sync point did not change
    assert_eq!(
        sync_point.ntp_time.read().unwrap().unwrap(),
        sync_point_ntp_time
    );

    drain_slew(&mut stream_1, stream_1_first_rtp_timestamp + 10_000);
    drain_slew(&mut stream_2, 5_000);

    let stream_1_second_pts_old = stream_1_second_pts;
    let stream_2_second_pts_old = stream_2_second_pts;
    let stream_1_second_pts = stream_1.pts_from_timestamp(stream_1_first_rtp_timestamp + 10_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(5_000);
    // check if stream_1 pts did not change
    assert_eq!(stream_1_second_pts, stream_1_second_pts_old);
    // check if stream_2 is the same as stream 1 for the same pts (no 100ms difference anymore)
    assert_eq!(stream_2_second_pts, stream_1_second_pts);
    // check if pts shifted by about 100ms (sleep time between initial packets)
    assert_duration_eq(
        stream_2_second_pts + Duration::from_millis(100),
        stream_2_second_pts_old,
        PREC_RUNTIME,
    );
}

#[test]
fn test_rtcp_sync_pts_with_rollover_after_sender_report_first_stream() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000);

    let stream_1_first_rtp_timestamp = 5_000;
    let stream_2_first_rtp_timestamp = 100_000;

    let stream_1_first_pts = stream_1.pts_from_timestamp(stream_1_first_rtp_timestamp);
    thread::sleep(Duration::from_millis(100));
    let stream_2_first_pts = stream_2.pts_from_timestamp(stream_2_first_rtp_timestamp);

    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::from_millis(100), PREC_RUNTIME);

    // after 10 seconds
    let stream_1_second_pts = stream_1.pts_from_timestamp(15_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(stream_2_first_rtp_timestamp + 10_000);

    assert_eq!(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(10)
    );
    assert_eq!(
        stream_2_second_pts,
        stream_2_first_pts + Duration::from_secs(10)
    );

    // provide sync information
    assert!(sync_point.ntp_time.read().unwrap().is_none());

    // could be any number larger than 60_000_000, otherwise we would return timestamp from
    // before sync point
    stream_1.on_sender_report(REFERENCE_NTP_TIME, u32::MAX - 5_000 + 1);

    let sync_point_ntp_time = sync_point.ntp_time.read().unwrap().unwrap();

    // stream_2 offset is one second relative to stream starts, but offset
    // between PTS values representing the same time is 29 second.
    stream_2.on_sender_report(REFERENCE_NTP_TIME, stream_2_first_rtp_timestamp - 10_000);

    // check is sync point did not change
    assert_eq!(
        sync_point.ntp_time.read().unwrap().unwrap(),
        sync_point_ntp_time
    );

    drain_slew(&mut stream_1, 15_000);
    drain_slew(&mut stream_2, stream_2_first_rtp_timestamp + 10_000);

    let stream_1_second_pts_old = stream_1_second_pts;
    let stream_2_second_pts_old = stream_2_second_pts;
    let stream_2_second_pts = stream_1.pts_from_timestamp(15_000);
    let stream_1_second_pts = stream_2.pts_from_timestamp(stream_2_first_rtp_timestamp + 10_000);
    // check if stream_1 pts did not change
    assert_eq!(stream_1_second_pts, stream_1_second_pts_old);
    // check if stream_2 is the same as stream 1 for the same pts (no 100ms difference anymore)
    assert_eq!(stream_2_second_pts, stream_1_second_pts);
    // check if pts shifted by about 100ms (sleep time between initial packets)
    assert_duration_eq(
        stream_2_second_pts + Duration::from_millis(100),
        stream_2_second_pts_old,
        PREC_RUNTIME,
    );
}

#[test]
fn test_rtcp_sync_pts_with_rollover_after_sender_report_second_stream() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000);

    let stream_1_first_rtp_timestamp = 100_000;
    let stream_2_first_rtp_timestamp = 5_000;

    let stream_1_first_pts = stream_1.pts_from_timestamp(stream_1_first_rtp_timestamp);
    thread::sleep(Duration::from_millis(100));
    let stream_2_first_pts = stream_2.pts_from_timestamp(stream_2_first_rtp_timestamp);

    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::from_millis(100), PREC_RUNTIME);

    // after 10 seconds
    let stream_1_second_pts = stream_1.pts_from_timestamp(stream_1_first_rtp_timestamp + 10_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(15_000);

    assert_eq!(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(10)
    );
    assert_eq!(
        stream_2_second_pts,
        stream_2_first_pts + Duration::from_secs(10)
    );

    // provide sync information
    assert!(sync_point.ntp_time.read().unwrap().is_none());

    // could be any number larger than 60_000_000, otherwise we would return timestamp from
    // before sync point
    stream_1.on_sender_report(REFERENCE_NTP_TIME, stream_1_first_rtp_timestamp - 10_000);

    let sync_point_ntp_time = sync_point.ntp_time.read().unwrap().unwrap();

    // stream_2 offset is one second relative to stream starts, but offset
    // between PTS values representing the same time is 29 second.
    stream_2.on_sender_report(REFERENCE_NTP_TIME, u32::MAX - 5_000 + 1);

    // check is sync point did not change
    assert_eq!(
        sync_point.ntp_time.read().unwrap().unwrap(),
        sync_point_ntp_time
    );

    drain_slew(&mut stream_1, stream_1_first_rtp_timestamp + 10_000);
    drain_slew(&mut stream_2, 15_000);

    let stream_1_second_pts_old = stream_1_second_pts;
    let stream_2_second_pts_old = stream_2_second_pts;
    let stream_1_second_pts = stream_1.pts_from_timestamp(stream_1_first_rtp_timestamp + 10_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(15_000);
    // check if stream_1 pts did not change
    assert_eq!(stream_1_second_pts, stream_1_second_pts_old);
    // check if stream_2 is the same as stream 1 for the same pts (no 100ms difference anymore)
    assert_eq!(stream_2_second_pts, stream_1_second_pts);
    // check if pts shifted by about 100ms (sleep time between initial packets)
    assert_duration_eq(
        stream_2_second_pts + Duration::from_millis(100),
        stream_2_second_pts_old,
        PREC_RUNTIME,
    );
}

/// Test that mismatched SR and media RTP timestamps (common with SFUs in WHEP)
/// don't produce negative PTS when the pipeline has been running for a while.
#[test]
fn test_rtcp_sync_rejects_mismatched_sr_timestamps() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 48_000);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 90_000);

    // Simulate pipeline running for ~20000 seconds before WHEP input starts
    // by using a large initial offset in NTP calculations.
    // First packets arrive "now" (sync_offset ≈ 0 since queue_sync_point = Instant::now())
    let stream_1_first_pts = stream_1.pts_from_timestamp(100_000_000);
    let stream_2_first_pts = stream_2.pts_from_timestamp(200_000_000);

    // Both should have PTS near 0 (since queue_sync_point is "now")
    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::ZERO, PREC_RUNTIME);

    // Simulate mismatched SR: SFU forwards original sender's SR with rtp_time
    // from a completely different timeline (sender running for hours).
    // Media rtp_ts=100M but SR rtp_time=2.1B (huge mismatch)
    stream_1.on_sender_report(REFERENCE_NTP_TIME, 2_100_000_000);

    // After the mismatched sync, subsequent packets should still have reasonable PTS
    // (the bad NTP offset should be rejected)
    let stream_1_second_pts = stream_1.pts_from_timestamp(100_048_000); // +1 second
    assert_duration_eq(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(1),
        PREC_RUNTIME,
    );

    // Stream 2 should also not be affected
    stream_2.on_sender_report(REFERENCE_NTP_TIME, 3_000_000_000);
    let stream_2_second_pts = stream_2.pts_from_timestamp(200_090_000); // +1 second
    assert_duration_eq(
        stream_2_second_pts,
        stream_2_first_pts + Duration::from_secs(1),
        PREC_RUNTIME,
    );
}

/// SR sets `target_offset_secs`; `pts_from_timestamp` then slews
/// `sync_offset_secs` toward it by ≤100µs per packet. To assert the converged
/// state (rather than the per-packet step), pump enough packets to drain any
/// outstanding diff. 1500 packets × 100µs = 150ms — covers any drift in these
/// tests. Once `current == target`, further calls clamp to a 0-step.
fn drain_slew(stream: &mut RtpTimestampSync, rtp_timestamp: u32) {
    for _ in 0..1500 {
        stream.pts_from_timestamp(rtp_timestamp);
    }
}

/// Verifies the slew mechanism itself: an SR sets a non-trivial target, after
/// which each `pts_from_timestamp` shifts the offset by exactly one step until
/// convergence.
///
/// Note: the FIRST SR ever can't produce a non-trivial diff — `ensure_sync_info`
/// builds the shared anchor from that SR + the first packet, which makes the
/// SR-derived offset equal to the current best-effort offset by construction.
/// We need a SECOND SR (after the anchor is fixed) with a different NTP/RTP
/// pairing for `target_offset_secs` to actually move.
#[test]
fn test_rtcp_sync_offset_slews_per_packet() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());
    let mut stream = RtpTimestampSync::new(sync_point.clone(), 1_000);

    stream.pts_from_timestamp(0);
    stream.pts_from_timestamp(1_000);
    let pts_pre_sr = stream.pts_from_timestamp(2_000);

    // First SR — establishes the shared anchor; target ends up matching current.
    stream.on_sender_report(REFERENCE_NTP_TIME, 0);

    // Second SR at the same RTP timestamp but with NTP shifted +50ms (sender
    // clock has drifted forward). `ensure_sync_info` is a no-op now, so the
    // +50ms lands in `target_offset_secs`.
    let shifted_ntp = REFERENCE_NTP_TIME + (POW_2_32 / 20); // +50 ms
    stream.on_sender_report(shifted_ntp, 0);

    // First packet after the second SR: shifts by exactly one slew step.
    let pts_after_one = stream.pts_from_timestamp(2_000);
    assert_duration_eq(
        pts_after_one - pts_pre_sr,
        Duration::from_micros(100),
        Duration::from_micros(10),
    );

    // After draining: full +50ms target reached.
    drain_slew(&mut stream, 2_000);
    let pts_after_drain = stream.pts_from_timestamp(2_000);
    assert_duration_eq(
        pts_after_drain - pts_pre_sr,
        Duration::from_millis(50),
        Duration::from_micros(200),
    );
}

fn assert_duration_eq(left: Duration, right: Duration, precision: Duration) {
    if left > right + precision || right > left + precision {
        panic!("{left:?} != right {right:?} (precision: {precision:?})")
    }
}
