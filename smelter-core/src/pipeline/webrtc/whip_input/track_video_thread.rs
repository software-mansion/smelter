use std::sync::Arc;

use tracing::warn;
use webrtc::{rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote};

use crate::pipeline::{
    decoder::negotiated_codecs::NegotiatedVideoCodecsInfo,
    rtp::RtpNtpSyncPoint,
    webrtc::{
        error::WhipWhepServerError, video_processing_loop::video_processing_loop,
        WhipWhepServerState,
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
        NegotiatedVideoCodecsInfo::from_webrtc_transceiver(transceiver, &video_preferences).await
    else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No video codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx, .. } = state;
    let frame_sender = inputs.get_with(&endpoint_id, |input| Ok(input.frame_sender.clone()))?;

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
