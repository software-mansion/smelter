use std::sync::Arc;

use tracing::debug;
use webrtc::track::track_remote::TrackRemote;

use crate::pipeline::{
    rtp::{RtpPacket, RtpTimestampSync},
    webrtc::input_rtcp_listener::RtcpListeners,
};

pub(super) struct WebrtcRtpReader {
    pub track: Arc<TrackRemote>,
    pub timestamp_sync: RtpTimestampSync,
    pub rtcp_listeners: RtcpListeners,
}

impl WebrtcRtpReader {
    pub async fn read_packet(&mut self) -> Option<RtpPacket> {
        let packet = match self.track.read_rtp().await {
            Ok((packet, _)) => packet,
            Err(err) => {
                debug!(?err, "Failed to read next RTP packet");
                return None;
            }
        };

        if let Ok(report) = self.rtcp_listeners.sender_report_receiver.try_recv() {
            self.timestamp_sync
                .on_sender_report(report.ntp_time, report.rtp_time);
        }
        let timestamp = self
            .timestamp_sync
            .pts_from_timestamp(packet.header.timestamp);

        Some(RtpPacket { packet, timestamp })
    }
}
