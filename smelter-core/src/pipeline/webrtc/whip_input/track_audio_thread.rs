use std::sync::Arc;

use tracing::warn;
use webrtc::{rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote};

use crate::pipeline::{
    decoder::negotiated_codecs::NegotiatedAudioCodecsInfo,
    rtp::RtpNtpSyncPoint,
    webrtc::{
        audio_processing_loop::audio_processing_loop, error::WhipWhepServerError,
        WhipWhepServerState,
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
    let Some(_negotiated_codecs) =
        NegotiatedAudioCodecsInfo::from_webrtc_transceiver(transceiver).await
    else {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No audio codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx, .. } = state;
    let samples_sender =
        inputs.get_with(&endpoint_id, |input| Ok(input.input_samples_sender.clone()))?;
    audio_processing_loop(
        ctx,
        sync_point,
        samples_sender,
        track,
        format!("WHIP input audio, endpoint_id: {}", endpoint_id).into(),
        rtc_receiver,
    )
    .await?;
    Ok(())
}
