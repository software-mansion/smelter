use std::sync::Arc;

use tracing::{debug, warn};
use webrtc::{
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
    rtp_transceiver::rtp_sender::RTCRtpSender,
};

use crate::PipelineCtx;

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
