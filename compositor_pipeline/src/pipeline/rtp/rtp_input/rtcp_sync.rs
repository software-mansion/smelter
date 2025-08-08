use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use tracing::{debug, warn};

use crate::pipeline::rtp::rtp_input::rollover_state::RolloverState;

const POW_2_32: f64 = (1i64 << 32) as f64;

#[derive(Debug)]
/// State that should be shared between different RTP tracks to use for synchronization.
pub(crate) struct RtpNtpSyncPoint {
    sync_point: Instant,
    /// First 32 bytes represent seconds, last 32 bytes fraction of the second.
    /// Represents NTP time of sync point
    ntp_time: RwLock<Option<u64>>,
}

impl RtpNtpSyncPoint {
    pub fn new(sync_point: Instant) -> Arc<Self> {
        Self {
            sync_point,
            ntp_time: RwLock::new(None),
        }
        .into()
    }

    fn ntp_time_to_pts(&self, ntp_time: u64) -> Duration {
        let sync_point_ntp_time = self.ntp_time.read().unwrap().unwrap_or(0) as i128;
        let ntp_time_diff_secs = (ntp_time as i128 - sync_point_ntp_time) as f64 / POW_2_32;
        Duration::try_from_secs_f64(ntp_time_diff_secs).unwrap_or_else(|err| {
            warn!(%err, "NTP time from before sync point");
            Duration::ZERO
        })
    }

    /// sr_ntp_time - NTP time from SenderReport
    /// rtp_timestamp - rtp timestamp from SenderReport (represents sr_ntp_time)
    /// reference_rtp_timestamp - rtp timestamp of some reference RTP packet
    /// reference_pts - pts(duration from sync_point without buffer) representing above packet
    fn ensure_sync_info(
        &self,
        sr_ntp_time: u64,
        sr_rtp_timestamp: u32,
        cmp_rtp_timestamp: u32,
        cmp_pts: Duration,
        clock_rate: u32,
    ) {
        {
            let guard = self.ntp_time.read().unwrap();
            if guard.is_some() {
                return;
            }
        }

        let mut guard = self.ntp_time.write().unwrap();
        let rtp_diff_secs =
            (cmp_rtp_timestamp as f64 - sr_rtp_timestamp as f64) / clock_rate as f64;

        let sync_point_ntp_time = sr_ntp_time as i128
            + (rtp_diff_secs * POW_2_32) as i128 // ntp time of cmp packet
            - (cmp_pts.as_secs_f64() * POW_2_32) as i128; // ntp_time of sync_point

        debug!(sync_point_ntp_time, "RTP synchronization point established");

        *guard = Some(sync_point_ntp_time as u64);
    }
}

/// To synchronize with NTP we need to have information about any RTP packet and SenderReport.
/// This struct is used to store partial state in the meantime.
#[derive(Debug)]
enum PartialNtpSyncInfo {
    Synced,
    None,
    ReferencePacket { rtp_timestamp: u32, pts: Duration },
    SenderReport { ntp_time: u64, rtp_timestamp: u32 },
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
    sync_offset: Option<Duration>,
    // additional buffer that defines how much input start should be ahead
    // of the queue.
    buffer_duration: Duration,
    clock_rate: u32,
    rollover_state: RolloverState,

    sync_point: Arc<RtpNtpSyncPoint>,
    partial_sync_info: PartialNtpSyncInfo,
}

impl RtpTimestampSync {
    pub fn new(
        sync_point: &Arc<RtpNtpSyncPoint>,
        clock_rate: u32,
        buffer_duration: Duration,
    ) -> Self {
        Self {
            sync_offset: None,
            rtp_timestamp_offset: None,
            buffer_duration,

            clock_rate,
            rollover_state: Default::default(),

            sync_point: sync_point.clone(),
            partial_sync_info: PartialNtpSyncInfo::None,
        }
    }

    pub fn pts_from_timestamp(&mut self, rtp_timestamp: u32) -> Duration {
        let sync_offset = *self.sync_offset.get_or_insert_with(|| {
            let sync_offset = self.sync_point.sync_point.elapsed();
            debug!(
                ?sync_offset,
                initial_rtp_timestamp = rtp_timestamp,
                "Init offset from sync point"
            );
            sync_offset
        });

        let rolled_timestamp = self.rollover_state.timestamp(rtp_timestamp);

        let rtp_timestamp_offset = *self.rtp_timestamp_offset.get_or_insert(rolled_timestamp);

        let timestamp = rolled_timestamp - rtp_timestamp_offset;
        let pts = Duration::from_secs_f64(timestamp as f64 / self.clock_rate as f64) + sync_offset;

        match self.partial_sync_info {
            PartialNtpSyncInfo::None => {
                self.partial_sync_info = PartialNtpSyncInfo::ReferencePacket { rtp_timestamp, pts }
            }
            PartialNtpSyncInfo::SenderReport {
                ntp_time: sr_ntp_time,
                rtp_timestamp: sr_rtp_timestamp,
            } => {
                self.update_sync_offset(sr_ntp_time, sr_rtp_timestamp, rtp_timestamp, pts);
            }
            _ => (),
        }

        pts + self.buffer_duration
    }

    pub fn on_sender_report(&mut self, ntp_time: u64, rtp_timestamp: u32) {
        match self.partial_sync_info {
            PartialNtpSyncInfo::None => {
                self.partial_sync_info = PartialNtpSyncInfo::SenderReport {
                    ntp_time,
                    rtp_timestamp,
                }
            }
            PartialNtpSyncInfo::ReferencePacket {
                rtp_timestamp: reference_rtp_timestamp,
                pts: reference_pts,
            } => {
                self.update_sync_offset(
                    ntp_time,
                    rtp_timestamp,
                    reference_rtp_timestamp,
                    reference_pts,
                );
            }
            _ => (),
        }
    }

    fn update_sync_offset(
        &mut self,
        sr_ntp_time: u64,
        sr_rtp_timestamp: u32,
        reference_rtp_timestamp: u32,
        reference_pts: Duration,
    ) {
        self.partial_sync_info = PartialNtpSyncInfo::Synced;
        self.sync_point.ensure_sync_info(
            sr_ntp_time,
            sr_rtp_timestamp,
            reference_rtp_timestamp,
            reference_pts,
            self.clock_rate,
        );

        // pts representing rtp timestamp from SenderReport
        let pts = self.sync_point.ntp_time_to_pts(sr_ntp_time);
        let pts_diff_secs = (reference_rtp_timestamp as i64 - sr_rtp_timestamp as i64) as f64
            / self.clock_rate as f64;

        let new_offset = Duration::from_secs_f64(pts.as_secs_f64() + pts_diff_secs);

        debug!(old_offset=?self.sync_offset, ?new_offset, "Updating RTP sync offset");

        self.sync_offset = Some(new_offset)
    }
}
