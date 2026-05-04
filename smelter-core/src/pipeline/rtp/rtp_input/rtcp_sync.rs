use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use tracing::{debug, warn};

use crate::pipeline::rtp::rtp_input::rollover_state::TimestampRolloverState;

#[cfg(test)]
mod sync_test;

const POW_2_32: f64 = (1i64 << 32) as f64;

/// Maximum amount `sync_offset_secs` is shifted toward `target_offset_secs` on
/// every media packet. Smooths out per-SR jumps so PTS values change
/// continuously instead of stepping on each SenderReport.
const MAX_OFFSET_INCREMENT: Duration = Duration::from_micros(100);

#[derive(Debug)]
/// State that should be shared between different RTP tracks to use for synchronization.
/// Whenever you create the sync point you should queue new track with
/// `QueueTrackOffset::Pts(Duration::ZERO)`
pub(crate) struct RtpNtpSyncPoint {
    pub reference_time: Instant,
    /// First 32 bits represent seconds, last 32 bits fraction of the second.
    /// Represents NTP time of a `reference_time`,
    ntp_time: RwLock<Option<u64>>,
}

impl RtpNtpSyncPoint {
    pub fn new(reference_time: Instant) -> Arc<Self> {
        Self {
            reference_time,
            ntp_time: RwLock::new(None),
        }
        .into()
    }

    fn ntp_time_to_pts_secs(&self, ntp_time: u64) -> f64 {
        let sync_point_ntp_time = self.ntp_time.read().unwrap().unwrap_or(0) as i128;
        (ntp_time as i128 - sync_point_ntp_time) as f64 / POW_2_32
    }

    /// Establishes the shared NTP anchor on the first call (no-op afterwards).
    ///
    /// - `sr_ntp_time`           — NTP time from the SenderReport.
    /// - `sr_rolled_rtp_timestamp` — SR's RTP timestamp, resolved into the same
    ///   rolled u64 space the calling track uses (so the diff against
    ///   `cmp_rolled_rtp_timestamp` is exact and rollover-free).
    /// - `cmp_rolled_rtp_timestamp` — rolled RTP timestamp of some reference
    ///   media packet on the calling track.
    /// - `cmp_pts_secs`          — that reference packet's PTS in the shared
    ///   timeframe (seconds from `reference_time`, ignoring buffering).
    /// - `clock_rate`            — of the calling track.
    fn ensure_sync_info(
        &self,
        sr_ntp_time: u64,
        sr_rolled_rtp_timestamp: u64,
        cmp_rolled_rtp_timestamp: u64,
        cmp_pts_secs: f64,
        clock_rate: u32,
    ) {
        {
            let guard = self.ntp_time.read().unwrap();
            if guard.is_some() {
                return;
            }
        }

        let mut guard = self.ntp_time.write().unwrap();
        if guard.is_some() {
            return;
        }
        let rtp_timestamp_diff = cmp_rolled_rtp_timestamp as f64 - sr_rolled_rtp_timestamp as f64;

        let rtp_diff_secs = rtp_timestamp_diff / clock_rate as f64;

        let sync_point_ntp_time = sr_ntp_time as i128
            + (rtp_diff_secs * POW_2_32) as i128 // ntp time of cmp packet
            - (cmp_pts_secs * POW_2_32) as i128; // ntp_time of sync_point

        debug!(sync_point_ntp_time, "RTP synchronization point established");

        *guard = Some(sync_point_ntp_time as u64);
    }
}

#[derive(Debug)]
pub(crate) struct RtpTimestampSync {
    // offset to sync timestamps to zero (and at the same time PTS of the first packet)
    rtp_timestamp_offset: Option<u64>,
    // offset to sync final duration to sync_point, assuming
    // that pts of first packet was zero.
    //
    // Calculation:
    // - best effort at start: elapsed since sync point on first packet
    // - after sync:
    //   - get pts of some packet from RtpNtpSyncPoint
    //   - calculate pts of first packet based on the difference
    //   - pts of first packet is an offset
    sync_offset_secs: Option<f64>,
    /// NTP-derived target for `sync_offset_secs`, refreshed on every
    /// SenderReport. `pts_from_timestamp` slews `sync_offset_secs` toward this
    /// value by at most `MAX_OFFSET_INCREMENT` per packet.
    target_offset_secs: Option<f64>,
    clock_rate: u32,
    rollover_state: TimestampRolloverState,

    ntp_sync_point: Arc<RtpNtpSyncPoint>,
    /// First media packet's `(rolled_rtp_timestamp, pts_secs)`. Set once on
    /// the first `pts_from_timestamp` call and never refreshed — used by
    /// `on_sender_report` as the fixed reference against which to recompute
    /// `sync_offset_secs`. The stored `pts_secs` is the best-effort estimate
    /// at first-packet time (= initial `sync_offset_secs`); it's only
    /// consumed by `ensure_sync_info` on the very first SR (no-op afterwards).
    first_reference_packet: Option<(u64, f64)>,
}

impl RtpTimestampSync {
    pub fn new(ntp_sync_point: Arc<RtpNtpSyncPoint>, clock_rate: u32) -> Self {
        Self {
            sync_offset_secs: None,
            target_offset_secs: None,
            rtp_timestamp_offset: None,

            clock_rate,
            rollover_state: Default::default(),

            ntp_sync_point,
            first_reference_packet: None,
        }
    }

    pub fn pts_from_timestamp(&mut self, rtp_timestamp: u32) -> Duration {
        let mut sync_offset_secs = *self.sync_offset_secs.get_or_insert_with(|| {
            let sync_offset = self.ntp_sync_point.reference_time.elapsed();
            debug!(
                ?sync_offset,
                initial_rtp_timestamp = rtp_timestamp,
                "Init offset from sync point"
            );
            sync_offset.as_secs_f64()
        });

        // Slew toward the latest NTP-derived target. Each packet shifts the
        // offset by at most MAX_OFFSET_INCREMENT so PTS values change
        // continuously instead of jumping on each SenderReport.
        if let Some(target) = self.target_offset_secs {
            let max_step_secs = MAX_OFFSET_INCREMENT.as_secs_f64();
            let nudge = (target - sync_offset_secs).clamp(-max_step_secs, max_step_secs);
            sync_offset_secs += nudge;
            self.sync_offset_secs = Some(sync_offset_secs);
        }

        let rolled_timestamp = self.rollover_state.timestamp(rtp_timestamp);
        let rtp_timestamp_offset = *self.rtp_timestamp_offset.get_or_insert(rolled_timestamp);

        if rtp_timestamp_offset > rolled_timestamp {
            warn!("RTP timestamp from before reference_time. Timestamp smaller than the offset.")
        }

        let timestamp = rolled_timestamp as f64 - rtp_timestamp_offset as f64;
        let pts_secs = (timestamp / self.clock_rate as f64) + sync_offset_secs;

        self.first_reference_packet
            .get_or_insert((rolled_timestamp, pts_secs));

        if pts_secs < 0.0 {
            warn!(pts_secs, "PTS from before queue start");
            Duration::ZERO
        } else {
            Duration::from_secs_f64(pts_secs)
        }
    }

    pub fn on_sender_report(&mut self, sr_ntp_time: u64, sr_rtp_timestamp: u32) {
        let Some((ref_rolled_rtp_timestamp, ref_pts_secs)) = self.first_reference_packet else {
            return;
        };

        // The value is rolled relative to recent timestamps not to the reference
        let sr_rolled_rtp_timestamp = self.rollover_state.timestamp(sr_rtp_timestamp);

        self.ntp_sync_point.ensure_sync_info(
            sr_ntp_time,
            sr_rolled_rtp_timestamp,
            ref_rolled_rtp_timestamp,
            ref_pts_secs,
            self.clock_rate,
        );

        // pts representing SenderReport (from ntp time we know pts, and that pts represents a
        // timestamp)
        let sr_pts_secs = self.ntp_sync_point.ntp_time_to_pts_secs(sr_ntp_time);

        let rtp_timestamp_diff = ref_rolled_rtp_timestamp as f64 - sr_rolled_rtp_timestamp as f64;
        // PTS of the ref packet calculated based on a new sender report. We are shifting
        // pts of a sender report by diff calculated from their rtp timestamps
        let new_ref_pts_secs = sr_pts_secs + rtp_timestamp_diff / self.clock_rate as f64;

        // because we use first packet as reference then new offset is the same as
        // pts of the first packet
        let new_offset_secs = new_ref_pts_secs;

        // Validate that the NTP-based offset is reasonable. We can hit that issue when:
        // - receiving stream from SFU that modifies RTP packet but not RTCP packets.
        // - BroadcastBox if you connect to server over WHEP before starting stream
        let offset_diff_secs = new_offset_secs - self.sync_offset_secs.unwrap_or(0.0);
        if offset_diff_secs.abs() > 2.0 {
            warn!(
                offset_diff_secs,
                "NTP sync offset differs too much from initial estimate, forcing update"
            );
            self.target_offset_secs = Some(new_offset_secs);
            self.sync_offset_secs = Some(new_offset_secs);
        } else {
            debug!(
                offset_diff_secs,
                old_target_offset_secs = ?self.target_offset_secs,
                new_target_offset_secs = new_offset_secs,
                current_offset_secs = ?self.sync_offset_secs,
                "Updating RTP sync offset target"
            );
            self.target_offset_secs = Some(new_offset_secs);
        }
    }
}
