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

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

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

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

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

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 3_000, true);

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

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

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

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

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

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

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

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

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

/// When an SR implies an offset more than `SNAP_THRESHOLD` away from the
/// current best-effort estimate (e.g. SFU mangling timestamps, or audio
/// resuming after a long pause), `sync_offset_secs` snaps to the new target
/// instead of slewing. After the snap, consecutive packets must still
/// advance by their RTP-time delta — i.e. the timeline stays self-consistent
/// past the discontinuity.
#[test]
fn test_rtcp_sync_snaps_on_large_offset_diff() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(sync_point.clone(), 48_000, true);
    let mut stream_2 = RtpTimestampSync::new(sync_point.clone(), 90_000, true);

    let stream_1_first_pts = stream_1.pts_from_timestamp(100_000_000);
    let stream_2_first_pts = stream_2.pts_from_timestamp(200_000_000);
    assert_duration_eq(stream_1_first_pts, Duration::ZERO, PREC_RUNTIME);
    assert_duration_eq(stream_2_first_pts, Duration::ZERO, PREC_RUNTIME);

    // Stream 1 SR builds the shared NTP anchor — by construction the SR-derived
    // offset matches the current best-effort, so no snap happens here.
    stream_1.on_sender_report(REFERENCE_NTP_TIME, 2_100_000_000);
    let stream_1_second_pts = stream_1.pts_from_timestamp(100_048_000); // +1 second
    assert_duration_eq(
        stream_1_second_pts,
        stream_1_first_pts + Duration::from_secs(1),
        PREC_RUNTIME,
    );

    // Stream 2 SR runs against the already-fixed anchor. Its (sr_rtp, ref_rtp)
    // pairing implies an offset ~58000s away from current → snap.
    stream_2.on_sender_report(REFERENCE_NTP_TIME, 3_000_000_000);
    let stream_2_post_snap_pts = stream_2.pts_from_timestamp(200_090_000); // +1 second
    let stream_2_next_pts = stream_2.pts_from_timestamp(200_180_000); // +2 seconds
    // Snap moved the timeline far away, but consecutive packets still differ
    // by the expected 1s.
    assert!(
        stream_2_post_snap_pts > stream_2_first_pts + Duration::from_secs(1000),
        "expected stream_2 to snap to a far-away offset, got {stream_2_post_snap_pts:?}"
    );
    assert_duration_eq(
        stream_2_next_pts - stream_2_post_snap_pts,
        Duration::from_secs(1),
        PREC_RUNTIME,
    );
}

/// SR sets `target_offset_secs`; `pts_from_timestamp` then slews
/// `sync_offset_secs` toward it by `CONVERGENCE_RATIO` of the inter-packet
/// RTP-time delta. To converge, packets must actually advance — same-timestamp
/// calls produce a zero step. We pump packets stepping by `clock_rate / 10`
/// (= 100ms of media per packet → ~1ms slew step), then issue `rtp_timestamp`
/// once at the end so subsequent assertions read at a known timestamp.
/// 3000 × 1ms = 3s of cumulative drain — covers any drift in these tests.
fn drain_slew(stream: &mut RtpTimestampSync, rtp_timestamp: u32) {
    let step = stream.clock_rate / 10;
    for i in 0..3000 {
        stream.pts_from_timestamp(rtp_timestamp.wrapping_add(i * step));
    }
    stream.pts_from_timestamp(rtp_timestamp);
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
    let mut stream = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

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

    // First packet after the second SR with RTP delta of 10 (= 10ms at clock
    // 1000). Slew step = 1% × 10ms = 100µs. The PTS also advances by the
    // intrinsic 10ms RTP delta, so isolate the slew contribution.
    let pts_after_one = stream.pts_from_timestamp(2_010);
    assert_duration_eq(
        pts_after_one - pts_pre_sr - Duration::from_millis(10),
        Duration::from_micros(100),
        Duration::from_micros(10),
    );

    // After draining: full +50ms target reached. Read back at rtp=2_000 to
    // line up with `pts_pre_sr`.
    drain_slew(&mut stream, 2_000);
    let pts_after_drain = stream.pts_from_timestamp(2_000);
    assert_duration_eq(
        pts_after_drain - pts_pre_sr,
        Duration::from_millis(50),
        Duration::from_micros(200),
    );
}

/// Sender pauses long enough to exceed the resume-skew threshold but keeps
/// RTP timestamps continuous on resume (Chrome WHEP mute/unmute behavior).
/// The first post-resume packet should snap `sync_offset_secs` forward by
/// the wall-clock/RTP-time skew so PTS reflects real time without waiting
/// for the next SR.
#[test]
fn test_rtcp_sync_snaps_on_sender_resume_without_rtp_gap() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());
    let mut stream = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

    let pts_first = stream.pts_from_timestamp(0);
    assert_duration_eq(pts_first, Duration::ZERO, PREC_RUNTIME);

    // Pause for 11s (above `RESUME_SKEW_SNAP_THRESHOLD` = 10s), then resume
    // with a continuous RTP timestamp (only +20 RTP units = 20ms of media).
    thread::sleep(Duration::from_secs(11));
    let pts_after_resume = stream.pts_from_timestamp(20);

    // Without the snap, PTS would be ≈ 20ms. With the snap, the wall-clock
    // gap (~11s) minus the RTP-time gap (20ms) gets added to sync_offset_secs,
    // so PTS lands near the real elapsed time.
    assert_duration_eq(
        pts_after_resume,
        Duration::from_secs(11),
        Duration::from_millis(20),
    );
}

/// When `real_time` is false (FixedWindow / buffered modes), a long
/// receiver block — e.g., a queue offset that delays consumption for
/// minutes — must NOT shift PTS. RTP timestamps in this mode carry media
/// time, so the post-block packet's PTS should reflect its RTP-time progress
/// only, regardless of how long the receiver was blocked. Same input that
/// would trigger a snap in the live-mode test stays untouched here.
#[test]
fn test_rtcp_sync_does_not_snap_when_not_real_time() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());
    let mut stream = RtpTimestampSync::new(sync_point.clone(), 1_000, false);

    let pts_first = stream.pts_from_timestamp(0);
    assert_duration_eq(pts_first, Duration::ZERO, PREC_RUNTIME);

    // Simulate the receiver being blocked for 11s (queue offset, slow
    // consumer, etc.) while sender's RTP timestamps stayed continuous in
    // media time. Must exceed `RESUME_SKEW_SNAP_THRESHOLD` (10s) so that the
    // test verifies the `real_time` guard, not just the threshold.
    thread::sleep(Duration::from_secs(11));
    let pts_after_block = stream.pts_from_timestamp(20);

    // No snap: PTS reflects the 20ms RTP-time advance, not the 11s wall gap.
    assert_duration_eq(pts_after_block, Duration::from_millis(20), PREC_RUNTIME);
}

/// A well-behaved sender that *does* advance RTP timestamps to reflect a
/// pause (rtp_gap ≈ wall_gap → skew ≈ 0) must not trip the resume snap.
#[test]
fn test_rtcp_sync_does_not_snap_when_rtp_gap_matches_wall_gap() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());
    let mut stream = RtpTimestampSync::new(sync_point.clone(), 1_000, true);

    let pts_first = stream.pts_from_timestamp(0);
    assert_duration_eq(pts_first, Duration::ZERO, PREC_RUNTIME);

    // Sender pauses 6s and advances RTP timestamps to match (6000 RTP
    // units at clock 1000 = 6s), so skew ≈ 0 — well below any threshold.
    thread::sleep(Duration::from_secs(6));
    let pts_after_resume = stream.pts_from_timestamp(6_000);

    // No snap: PTS comes purely from RTP-time advance.
    assert_duration_eq(
        pts_after_resume,
        Duration::from_secs(6),
        Duration::from_millis(20),
    );

    // And the next packet keeps stepping at the RTP rate, not at any
    // snapped offset.
    let pts_next = stream.pts_from_timestamp(7_000);
    assert_duration_eq(
        pts_next - pts_after_resume,
        Duration::from_secs(1),
        PREC_RUNTIME,
    );
}

fn assert_duration_eq(left: Duration, right: Duration, precision: Duration) {
    if left > right + precision || right > left + precision {
        panic!("{left:?} != right {right:?} (precision: {precision:?})")
    }
}
