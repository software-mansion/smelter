use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use tracing::{debug, warn};

use crate::pipeline::rtp::rtp_input::rollover_state::TimestampRolloverState;

#[cfg(test)]
mod sync_test;

const POW_2_32: f64 = (1i64 << 32) as f64;

/// Per-packet share of the (target − current) offset that `sync_offset_secs`
/// is slewed by. Sized to the inter-packet RTP-time delta so convergence
/// speed scales with media time rather than packet count, making it
/// bitrate-independent.
const CONVERGENCE_RATIO: f64 = 0.01;

/// If a fresh SR implies an offset more than this far from the current
/// best-effort, snap instead of slewing. Catches cases the slew can't recover
/// from in reasonable time (SFU rewriting RTP but not RTCP, or audio
/// resuming after a long pause).
const SNAP_THRESHOLD: Duration = Duration::from_millis(300);

/// In wall-clock-aligned modes (RealTime), if wall-clock advance between two
/// forward-moving packets exceeds the inter-packet RTP-time advance by more
/// than this, snap `sync_offset_secs` forward by the skew. Catches
/// sender-side resume after a real-time gap the sender failed to reflect in
/// RTP timestamps (Chrome WHEP on mute/unmute). Only enabled for sources
/// whose RTP timestamps are intended to track wall clock — buffered/file
/// sources (FixedWindow mode) intentionally use media-time RTP and must not
/// be snapped.
const RESUME_SKEW_SNAP_THRESHOLD: Duration = Duration::from_secs(10);

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
        Self { reference_time, ntp_time: RwLock::new(None) }.into()
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
        let rtp_timestamp_diff =
            cmp_rolled_rtp_timestamp as f64 - sr_rolled_rtp_timestamp as f64;

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
    /// value by at most `CONVERGENCE_RATIO` of the inter-packet RTP-time delta
    /// per packet.
    target_offset_secs: Option<f64>,
    /// Largest rolled RTP timestamp observed so far. Used to size the slew
    /// step from the inter-packet RTP-time delta — packets arriving out of
    /// order produce a zero step rather than nudging backwards.
    last_max_rolled_rtp_timestamp: Option<u64>,
    /// Wall-clock receive time paired with `last_max_rolled_rtp_timestamp`.
    /// Used by the resume-skew snap to detect sender-side resume after a
    /// real-time gap.
    last_max_recv_time: Option<Instant>,
    /// Whether the source is treated as live / wall-clock-aligned. Enables
    /// the resume-skew snap (RTP timestamps are expected to track wall
    /// clock). Cleared for buffered sources where RTP carries media time
    /// and a long receiver block must not shift PTS.
    real_time: bool,
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
    pub fn new(
        ntp_sync_point: Arc<RtpNtpSyncPoint>,
        clock_rate: u32,
        real_time: bool,
    ) -> Self {
        Self {
            sync_offset_secs: None,
            target_offset_secs: None,
            last_max_rolled_rtp_timestamp: None,
            last_max_recv_time: None,
            real_time,
            rtp_timestamp_offset: None,

            clock_rate,
            rollover_state: Default::default(),

            ntp_sync_point,
            first_reference_packet: None,
        }
    }

    pub fn pts_from_timestamp(&mut self, rtp_timestamp: u32) -> Duration {
        let rolled_timestamp = self.rollover_state.timestamp(rtp_timestamp);

        // Detect sender-side resume after a wall-clock gap the sender did not
        // reflect in RTP timestamps (Chrome WHEP on mute/unmute keeps RTP
        // continuous). When wall-clock advance outpaces RTP-time advance by
        // more than RESUME_SKEW_SNAP_THRESHOLD, snap sync_offset_secs forward
        // immediately so PTS lines up with wall clock — otherwise the next
        // SR is up to ~5s away and the buffer would balloon in the meantime.
        //
        // Gated on `real_time` because the heuristic is only valid when RTP
        // timestamps are intended to track wall clock. For buffered/file
        // inputs RTP carries media time and a long receiver block — e.g., a
        // queue offset that delays consumption — would otherwise be mistaken
        // for a sender-side resume and shift PTS by the block duration.
        self.maybe_snap_on_resume(rolled_timestamp);

        // Slew toward the latest NTP-derived target. Step size is
        // CONVERGENCE_RATIO * inter-packet RTP-time delta, so convergence
        // happens at a fixed rate per second of media regardless of bitrate.
        // Out-of-order packets (current < last_max) produce a zero step.
        self.maybe_converge_on_target(rolled_timestamp);

        let sync_offset_secs = *self.sync_offset_secs.get_or_insert_with(|| {
            let sync_offset = self.ntp_sync_point.reference_time.elapsed();
            debug!(
                ?sync_offset,
                initial_rtp_timestamp = rtp_timestamp,
                "Init offset from sync point"
            );
            sync_offset.as_secs_f64()
        });

        if rolled_timestamp > self.last_max_rolled_rtp_timestamp.unwrap_or(0) {
            self.last_max_rolled_rtp_timestamp = Some(rolled_timestamp);
        }
        self.last_max_recv_time = Some(Instant::now());

        let rtp_timestamp_offset =
            *self.rtp_timestamp_offset.get_or_insert(rolled_timestamp);

        if rtp_timestamp_offset > rolled_timestamp {
            warn!(
                "RTP timestamp from before reference_time. Timestamp smaller than the offset."
            )
        }

        let timestamp = rolled_timestamp as f64 - rtp_timestamp_offset as f64;
        let pts_secs = (timestamp / self.clock_rate as f64) + sync_offset_secs;

        self.first_reference_packet.get_or_insert((rolled_timestamp, pts_secs));

        if pts_secs < 0.0 {
            warn!(pts_secs, "PTS from before queue start");
            Duration::ZERO
        } else {
            Duration::from_secs_f64(pts_secs)
        }
    }

    /// Implementation of the slew toward `target_offset_secs`. Mutates
    /// `self.sync_offset_secs` in place by at most
    /// `CONVERGENCE_RATIO * inter-packet RTP-time delta`. No-op when no
    /// target is set or `sync_offset_secs` hasn't been initialized — in the
    /// latter case there's nothing to slew yet, the first packet sets the
    /// initial value downstream.
    fn maybe_converge_on_target(&mut self, rolled_timestamp: u64) {
        let (Some(target), Some(sync_offset_secs)) =
            (self.target_offset_secs, self.sync_offset_secs)
        else {
            return;
        };
        let last_max = self.last_max_rolled_rtp_timestamp.unwrap_or(rolled_timestamp);
        let rtp_delta_secs =
            rolled_timestamp.saturating_sub(last_max) as f64 / self.clock_rate as f64;
        let max_step_secs = rtp_delta_secs * CONVERGENCE_RATIO;
        let new_sync_offset_secs = target
            .clamp(sync_offset_secs - max_step_secs, sync_offset_secs + max_step_secs);
        self.sync_offset_secs = Some(new_sync_offset_secs);
    }

    /// Implementation of the resume-skew snap. Mutates `self.sync_offset_secs`
    /// and `self.target_offset_secs` in place when the skew exceeds the
    /// threshold; otherwise no-op. Both `sync_offset_secs` and
    /// `target_offset_secs` are pinned to the snapped value so the slew that
    /// runs next can't drag us back to a stale target before the next SR
    /// arrives. Safe to call before `sync_offset_secs` is initialized — the
    /// early-out on `last_max_recv_time` / `sync_offset_secs` ensures the
    /// snap can only fire after at least one prior packet has set them.
    fn maybe_snap_on_resume(&mut self, rolled_timestamp: u64) {
        if !self.real_time {
            return;
        }
        let (Some(prev_recv_time), Some(prev_rolled), Some(sync_offset_secs)) = (
            self.last_max_recv_time,
            self.last_max_rolled_rtp_timestamp,
            self.sync_offset_secs,
        ) else {
            return;
        };
        if rolled_timestamp <= prev_rolled {
            return;
        }

        let wall_gap_secs = prev_recv_time.elapsed().as_secs_f64();
        let rtp_gap_secs =
            (rolled_timestamp - prev_rolled) as f64 / self.clock_rate as f64;
        let skew_secs = wall_gap_secs - rtp_gap_secs;
        if skew_secs <= RESUME_SKEW_SNAP_THRESHOLD.as_secs_f64() {
            return;
        }

        warn!(
            skew_secs,
            "Sender resumed without RTP-time gap, snapping sync offset forward"
        );
        let new_sync_offset = sync_offset_secs + skew_secs;
        self.sync_offset_secs = Some(new_sync_offset);
        self.target_offset_secs = Some(new_sync_offset);
    }

    pub fn on_sender_report(&mut self, sr_ntp_time: u64, sr_rtp_timestamp: u32) {
        let Some((ref_rolled_rtp_timestamp, ref_pts_secs)) = self.first_reference_packet
        else {
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

        let rtp_timestamp_diff =
            ref_rolled_rtp_timestamp as f64 - sr_rolled_rtp_timestamp as f64;
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
        if offset_diff_secs.abs() > SNAP_THRESHOLD.as_secs_f64() {
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
