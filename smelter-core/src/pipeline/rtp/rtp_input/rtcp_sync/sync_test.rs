use std::{
    thread,
    time::{Duration, Instant},
};

use crate::pipeline::rtp::{RtpNtpSyncPoint, RtpTimestampSync};

const POW_2_32: u64 = 1u64 << 32;

// Allowed error that can be cause by speed of the runtime execution e.g. sleep precision
const PREC_RUNTIME: Duration = Duration::from_millis(5);

// Represents 09/09/2025, 1:00:21 PM
const REFERENCE_NTP_TIME: u64 = 3966409461 * POW_2_32;

#[test]
fn test_rtcp_sync_pts_from_zero() {
    let sync_point = RtpNtpSyncPoint::new(Instant::now());

    let mut stream_1 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);
    let mut stream_2 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);

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

    let mut stream_1 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);
    let mut stream_2 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);

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

    let mut stream_1 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);
    let mut stream_2 = RtpTimestampSync::new(&sync_point, 3_000, Duration::ZERO);

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

    let mut stream_1 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);
    let mut stream_2 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);

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

    let mut stream_1 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);
    let mut stream_2 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);

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

    let mut stream_1 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);
    let mut stream_2 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);

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

    let mut stream_1 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);
    let mut stream_2 = RtpTimestampSync::new(&sync_point, 1_000, Duration::ZERO);

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

fn assert_duration_eq(left: Duration, right: Duration, precision: Duration) {
    if left > right + precision || right > left + precision {
        panic!("{left:?} != right {right:?} (precision: {precision:?})")
    }
}
