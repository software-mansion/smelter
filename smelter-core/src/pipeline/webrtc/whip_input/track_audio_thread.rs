use std::sync::Arc;

use tracing::warn;
use webrtc::{rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote};

use crate::pipeline::{
    rtp::RtpNtpSyncPoint,
    webrtc::{
        WhipWhepServerState,
        audio_processing_loop::{AudioTrackCtx, audio_processing_loop},
        error::WhipWhepServerError,
        negotiated_codecs::audio_codec_negotiated,
    },
};

pub async fn process_audio_track(
    sync_point: Arc<RtpNtpSyncPoint>,
    state: WhipWhepServerState,
    endpoint_id: Arc<str>,
    track: Arc<TrackRemote>,
    transceiver: Arc<RTCRtpTransceiver>,
) -> Result<(), WhipWhepServerError> {
    let rtc_receiver = transceiver.receiver().await;
    if !audio_codec_negotiated(rtc_receiver.clone()).await {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No audio codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx, .. } = state;
    let samples_sender =
        inputs.get_with(&endpoint_id, |input| Ok(input.input_samples_sender.clone()))?;

    let audio_track_ctx = AudioTrackCtx {
        sync_point,
        track,
        samples_sender,
        rtc_receiver,
    };

    audio_processing_loop(
        ctx,
        audio_track_ctx,
        format!("WHIP input audio, endpoint_id: {}", endpoint_id).into(),
    )
    .await?;
    Ok(())
}
