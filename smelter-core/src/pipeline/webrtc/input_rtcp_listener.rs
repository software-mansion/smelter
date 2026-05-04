use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};
use webrtc::{
    rtcp::{self, sender_report::SenderReport},
    rtp_transceiver::rtp_receiver::RTCRtpReceiver,
};

use crate::PipelineCtx;

pub(super) struct RtcpListeners {
    pub sender_report_receiver: mpsc::UnboundedReceiver<SenderReport>,
}

impl RtcpListeners {
    pub(super) fn start(ctx: &Arc<PipelineCtx>, rtc_receiver: Arc<RTCRtpReceiver>) -> Self {
        let (sender_report_sender, sender_report_receiver) = mpsc::unbounded_channel();
        ctx.tokio_rt.spawn(async move {
            loop {
                match rtc_receiver.read_rtcp().await {
                    Ok((packets, _attr)) => {
                        for packet in packets {
                            debug!(?packet, "Received RTCP packet");
                            if packet.header().packet_type == rtcp::header::PacketType::SenderReport
                            {
                                let report = packet
                                    .as_any()
                                    .downcast_ref::<SenderReport>()
                                    .unwrap()
                                    .clone();
                                if let Err(err) = sender_report_sender.send(report) {
                                    warn!(%err, "Error while forwarding SenderReport.");
                                    return;
                                }
                            }
                        }
                    }
                    Err(webrtc::Error::ErrClosedPipe) => return,
                    Err(err) => {
                        warn!(%err, "Error while reading RTCP packet.");
                        return;
                    }
                }
            }
        });
        Self {
            sender_report_receiver,
        }
    }
}
