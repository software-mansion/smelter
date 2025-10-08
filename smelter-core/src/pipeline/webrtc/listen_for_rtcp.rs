use std::sync::Arc;

use tokio::sync::oneshot::Sender;
use tracing::{debug, warn};
use webrtc::{
    rtcp::{self, sender_report::SenderReport},
    rtp_transceiver::rtp_receiver::RTCRtpReceiver,
};

use crate::PipelineCtx;

pub(super) fn listen_for_rtcp(
    ctx: &Arc<PipelineCtx>,
    rtc_receiver: Arc<RTCRtpReceiver>,
    sender_report_sender: Sender<SenderReport>,
) {
    ctx.tokio_rt.spawn(async move {
        let mut sender = Some(sender_report_sender);
        loop {
            match rtc_receiver.read_rtcp().await {
                Ok((packets, _attr)) => {
                    for packet in packets {
                        debug!(?packet, "Received RTCP packet");
                        if packet.header().packet_type == rtcp::header::PacketType::SenderReport
                            && let Some(sender) = sender.take()
                        {
                            let result = sender.send(
                                packet
                                    .as_any()
                                    .downcast_ref::<SenderReport>()
                                    .unwrap()
                                    .clone(),
                            );
                            if let Err(err) = result {
                                warn!(%err, "Error while reading SenderReport.");
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
}
