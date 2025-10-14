use std::sync::Arc;

use tokio::sync::oneshot::Sender;
use tracing::{debug, warn};
use webrtc::{
    rtcp::{
        self, payload_feedbacks::picture_loss_indication::PictureLossIndication,
        sender_report::SenderReport,
    },
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, rtp_sender::RTCRtpSender},
};

use crate::PipelineCtx;

pub(super) fn listen_for_sender_reports(
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

pub(crate) fn handle_keyframe_requests(
    ctx: &Arc<PipelineCtx>,
    sender: Arc<RTCRtpSender>,
    keyframe_sender: crossbeam_channel::Sender<()>,
) {
    ctx.tokio_rt.spawn(async move {
        loop {
            if let Ok((packets, _)) = sender.read_rtcp().await {
                for packet in packets {
                    if packet
                        .as_any()
                        .downcast_ref::<PictureLossIndication>()
                        .is_some()
                    {
                        debug!("Request keyframe");
                        if let Err(err) = keyframe_sender.send(()) {
                            warn!(%err, "Failed to send keyframe request to the encoder.");
                            return;
                        };
                    }
                }
            } else {
                debug!("Failed to read RTCP packets from the sender.");
                return;
            }
        }
    });
}
