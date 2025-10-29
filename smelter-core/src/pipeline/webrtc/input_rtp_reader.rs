use std::{collections::BTreeMap, sync::Arc, time::Duration};

use tracing::{debug, trace, warn};
use webrtc::track::track_remote::TrackRemote;

use crate::pipeline::{
    rtp::{RtpPacket, RtpTimestampSync},
    webrtc::input_rtcp_listener::RtcpListeners,
};

pub(super) struct WebrtcRtpReader {
    pub track: Arc<TrackRemote>,
    pub timestamp_sync: RtpTimestampSync,
    pub rtcp_listeners: RtcpListeners,
    pub state: State,
}

pub(super) struct State {
    pub jitter_buffer: BTreeMap<u16, RtpPacket>,
    pub last_returned_seq: Option<u16>,
    pub last_received_seq: Option<u16>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            jitter_buffer: Default::default(),
            last_returned_seq: Default::default(),
            last_received_seq: Default::default(),
        }
    }
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

        trace!(seq = packet.header.sequence_number, ?timestamp, "Receiving");
        if let Some(last_send) = self.state.last_returned_seq
            && last_send > packet.header.sequence_number
        {
            warn!(
                last_send,
                seq = packet.header.sequence_number,
                "Received packet to old"
            );
        } else {
            if let Some(last_received) = self.state.last_received_seq {
                if last_received > packet.header.sequence_number {
                    warn!(
                        last_received,
                        seq = packet.header.sequence_number,
                        "Received out of order packet"
                    );
                }
            }
            self.state.last_received_seq = Some(packet.header.sequence_number);
            self.state.jitter_buffer.insert(
                packet.header.sequence_number,
                RtpPacket { packet, timestamp },
            );
        }

        let mut packets = vec![];
        loop {
            let Some(entry) = self.state.jitter_buffer.first_entry() else {
                break;
            };
            let queue_now = self.timestamp_sync.sync_point.sync_point.elapsed();

            if let Some(last_packet) = self.state.last_returned_seq {
                if last_packet > entry.get().packet.header.sequence_number {
                    entry.remove();
                    continue;
                }
            }

            if Some(entry.key() - 1) == self.state.last_returned_seq {
                // warn!(seq = entry.get().packet.header.sequence_number, timestamp=?entry.get().timestamp, ?queue_now, "Sending");
                self.state.last_returned_seq = Some(entry.get().packet.header.sequence_number);
                packets.push(entry.remove());
                continue;
            }

            if entry.get().timestamp < queue_now + Duration::from_millis(40) {
                let seq = entry.get().packet.header.sequence_number;
                warn!(
                    last_seq = self.state.last_returned_seq,
                    seq, "Missing packet"
                );
                // warn!(seq = entry.get().packet.header.sequence_number, timestamp=?entry.get().timestamp, ?queue_now, "Sending");
                self.state.last_returned_seq = Some(seq);
                packets.push(entry.remove());
                continue;
            }

            break;
        }
        // warn!(len = packets.len(), "stop");

        Some(packets)
    }
}
