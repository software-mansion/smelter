use std::sync::Arc;

use crossbeam_channel::Sender;
use tracing::warn;
use webrtc::{rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote};

use crate::pipeline::{
    rtp::RtpNtpSyncPoint,
    webrtc::{
        audio_processing_loop::audio_processing_loop, negotiated_codecs::NegotiatedAudioCodecsInfo,
    },
    PipelineCtx,
};

use crate::prelude::*;

pub async fn process_audio_track(
    ctx: Arc<PipelineCtx>,
    sync_point: Arc<RtpNtpSyncPoint>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    track: Arc<TrackRemote>,
    transceiver: Arc<RTCRtpTransceiver>,
) -> Result<(), WebrtcClientError> {
    let rtc_receiver = transceiver.receiver().await;
    let Some(_negotiated_codecs) = NegotiatedAudioCodecsInfo::new(transceiver).await else {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WebrtcClientError::NoAudioCodecNegotiated);
    };

    audio_processing_loop(
        ctx,
        sync_point,
        samples_sender,
        track,
        "WHEP input audio".into(),
        rtc_receiver,
    )
    .await?;

    Ok(())
}
