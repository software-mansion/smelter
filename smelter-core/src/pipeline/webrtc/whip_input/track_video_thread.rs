use std::sync::Arc;

use tracing::warn;
use webrtc::{rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote};

use crate::pipeline::{
    decoder::VideoDecoderMapping,
    rtp::{RtpNtpSyncPoint, depayloader::VideoPayloadTypeMapping},
    webrtc::{
        WhipWhepServerState,
        error::WhipWhepServerError,
        negotiated_codecs::{
            VideoCodecMappings, WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping,
        },
        video_processing_loop::{VideoTrackCtx, video_processing_loop},
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
    let (Some(decoder_mapping), Some(payload_type_mapping)) = (
        VideoDecoderMapping::from_webrtc_transceiver(transceiver.clone(), &video_preferences).await,
        VideoPayloadTypeMapping::from_webrtc_transceiver(transceiver).await,
    ) else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No video codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx, .. } = state;
    let frame_sender = inputs.get_with(&endpoint_id, |input| Ok(input.frame_sender.clone()))?;

    let video_mappings = VideoCodecMappings {
        decoder_mapping,
        payload_type_mapping,
    };

    let video_track_ctx = VideoTrackCtx {
        sync_point,
        track,
        frame_sender,
        rtc_receiver,
    };

    video_processing_loop(
        ctx,
        video_track_ctx,
        format!("WHIP input video, endpoint_id: {}", endpoint_id).into(),
        video_mappings,
    )
    .await?;

    Ok(())
}
