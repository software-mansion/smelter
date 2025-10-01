use std::sync::Arc;

use tracing::warn;
use webrtc::{
    rtcp::{packet::Packet, payload_feedbacks::picture_loss_indication::PictureLossIndication},
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, RTCRtpTransceiver},
    track::track_remote::TrackRemote,
};

use crate::pipeline::{
    rtp::RtpNtpSyncPoint,
    webrtc::{
        error::WhipWhepServerError, negotiated_codecs::NegotiatedVideoCodecsInfo,
        video_processing_loop::video_processing_loop, WhipWhepServerState,
    },
};

use crate::prelude::*;

pub async fn process_video_track(
    sync_point: Arc<RtpNtpSyncPoint>,
    state: WhipWhepServerState,
    endpoint_id: Arc<str>,
    track: Arc<TrackRemote>,
    transceiver: Arc<RTCRtpTransceiver>,
    video_preferences: Vec<VideoDecoderOptions>,
) -> Result<(), WhipWhepServerError> {
    let rtc_receiver = transceiver.receiver().await;
    let Some(negotiated_codecs) =
        NegotiatedVideoCodecsInfo::new(transceiver, &video_preferences).await
    else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No video codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx, .. } = state;
    let frame_sender = inputs.get_with(&endpoint_id, |input| Ok(input.frame_sender.clone()))?;
    request_keyframe(&track, &rtc_receiver).await?;
    video_processing_loop(
        ctx,
        sync_point,
        frame_sender,
        track,
        format!("WHIP input video, endpoint_id: {}", endpoint_id).into(),
        rtc_receiver,
        negotiated_codecs,
    )
    .await?;
    Ok(())
}

async fn request_keyframe(
    track: &Arc<TrackRemote>,
    rtc_receiver: &Arc<RTCRtpReceiver>,
) -> Result<usize, webrtc::Error> {
    let ssrc = track.ssrc();
    let pli = PictureLossIndication {
        // For receive-only endpoints RTP sender SSRC can be set to 0.
        sender_ssrc: 0,
        media_ssrc: ssrc,
    };

    let rtcp_packets: Vec<Box<dyn Packet + Send + Sync>> = vec![Box::new(pli)];
    rtc_receiver.transport().write_rtcp(&rtcp_packets).await
}
