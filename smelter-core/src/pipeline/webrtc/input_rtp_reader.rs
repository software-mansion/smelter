use std::{collections::BTreeMap, sync::Arc, time::Duration};

use tracing::{debug, warn};
use webrtc::track::track_remote::TrackRemote;

use crate::pipeline::{
    rtp::{RtpPacket, RtpTimestampSync},
    webrtc::input_rtcp_listener::RtcpListeners,
};

pub(super) struct WebrtcRtpReader {
    pub track: Arc<TrackRemote>,
    pub timestamp_sync: RtpTimestampSync,
    pub rtcp_listeners: RtcpListeners,
    pub jitter_buffer: BTreeMap<u16, RtpPacket>,
    pub last_returned_seq: Option<u16>,
}

impl WebrtcRtpReader {
    pub async fn read_packet(&mut self) -> Option<Vec<RtpPacket>> {
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

        warn!(seq = packet.header.sequence_number, ?timestamp, "Receiving");
        self.jitter_buffer.insert(
            packet.header.sequence_number,
            RtpPacket { packet, timestamp },
        );

        let mut packets = vec![];
        loop {
            let Some(entry) = self.jitter_buffer.first_entry() else {
                break;
            };
            let queue_now = self.timestamp_sync.sync_point.sync_point.elapsed();

            if let Some(last_packet) = self.last_returned_seq {
                if last_packet > entry.get().packet.header.sequence_number {
                    entry.remove();
                    continue;
                }
            }

            if entry.get().timestamp < queue_now + Duration::from_millis(40)
                || Some(entry.key() - 1) == self.last_returned_seq
            {
                warn!(seq = entry.get().packet.header.sequence_number, timestamp=?entry.get().timestamp, ?queue_now, "Sending");
                self.last_returned_seq = Some(entry.get().packet.header.sequence_number);
                packets.push(entry.remove());
                continue;
            }

            break;
        }
        warn!(len = packets.len(), "stop");

        Some(packets)
    }
}
