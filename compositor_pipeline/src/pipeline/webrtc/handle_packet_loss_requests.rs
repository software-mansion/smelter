use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use tracing::{debug, error, span, trace, Instrument, Level};
use webrtc::{
    peer_connection::RTCPeerConnection, rtp_transceiver::rtp_sender::RTCRtpSender,
    stats::StatsReportType,
};

use crate::PipelineCtx;

// Identifiers used in stats HashMap returnet by RTCPeerConnection::get_stats()
const RTC_OUTBOUND_RTP_AUDIO_STREAM: &str = "RTCOutboundRTPAudioStream_";
const RTC_REMOTE_INBOUND_RTP_AUDIO_STREAM: &str = "RTCRemoteInboundRTPAudioStream_";

pub(crate) fn handle_packet_loss_requests(
    ctx: &Arc<PipelineCtx>,
    pc: Arc<RTCPeerConnection>,
    rtc_sender: Arc<RTCRtpSender>,
    packet_loss_sender: watch::Sender<i32>,
    ssrc: u32,
) {
    let mut cumulative_packets_sent: u64 = 0;
    let mut cumulative_packets_lost: u64 = 0;

    let span = span!(Level::DEBUG, "Packet loss handle");

    ctx.tokio_rt.spawn(
        async move {
            loop {
                if let Err(e) = rtc_sender.read_rtcp().await {
                    debug!(%e, "Error while reading rtcp.");
                }
            }
        }
        .instrument(span.clone()),
    );

    ctx.tokio_rt.spawn(
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let stats = pc.get_stats().await.reports;
                let outbound_id = String::from(RTC_OUTBOUND_RTP_AUDIO_STREAM) + &ssrc.to_string();
                let remote_inbound_id =
                    String::from(RTC_REMOTE_INBOUND_RTP_AUDIO_STREAM) + &ssrc.to_string();

                let outbound_stats = match stats.get(&outbound_id) {
                    Some(StatsReportType::OutboundRTP(report)) => report,
                    Some(_) => {
                        error!("Invalid report type for given key! (This should not happen)");
                        continue;
                    }
                    None => {
                        debug!("OutboundRTP report is empty!");
                        continue;
                    }
                };

                let remote_inbound_stats = match stats.get(&remote_inbound_id) {
                    Some(StatsReportType::RemoteInboundRTP(report)) => report,
                    Some(_) => {
                        error!("Invalid report type for given key! (This should not happen)");
                        continue;
                    }
                    None => {
                        debug!("RemoteInboundRTP report is empty!");
                        continue;
                    }
                };

                let packets_sent: u64 = outbound_stats.packets_sent;
                // This can be lower than 0 in case of duplicates
                let packets_lost: u64 = i64::max(remote_inbound_stats.packets_lost, 0) as u64;

                let packet_loss_percentage = calculate_packet_loss_percentage(
                    packets_sent,
                    packets_lost,
                    cumulative_packets_sent,
                    cumulative_packets_lost,
                );
                if packet_loss_sender.send(packet_loss_percentage).is_err() {
                    debug!("Packet loss channel closed.");
                }
                cumulative_packets_sent = packets_sent;
                cumulative_packets_lost = packets_lost;
            }
        }
        .instrument(span),
    );
}

fn calculate_packet_loss_percentage(
    packets_sent: u64,
    packets_lost: u64,
    cumulative_packets_sent: u64,
    cumulative_packets_lost: u64,
) -> i32 {
    let packets_sent_since_last_report = packets_sent - cumulative_packets_sent;
    let packets_lost_since_last_report = packets_lost - cumulative_packets_lost;

    // I don't want the system to panic in case of some bug
    let packet_loss_percentage: i32 = if packets_sent_since_last_report != 0 {
        let mut loss =
            100.0 * packets_lost_since_last_report as f64 / packets_sent_since_last_report as f64;
        // loss is rounded up to the nearest multiple of 5
        loss = f64::ceil(loss / 5.0) * 5.0;
        loss as i32
    } else {
        0
    };

    trace!(
        packets_sent_since_last_report,
        packets_lost_since_last_report,
        packet_loss_percentage,
    );
    packet_loss_percentage
}
