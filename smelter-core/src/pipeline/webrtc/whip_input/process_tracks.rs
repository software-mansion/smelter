use std::sync::Arc;

use tracing::warn;
use webrtc::{rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote};

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::{
        decoder::VideoDecoderMapping,
        rtp::{RtpNtpSyncPoint, depayloader::VideoPayloadTypeMapping},
        webrtc::{
            WhipWhepServerState,
            audio_input_processing_loop::{AudioInputLoop, AudioTrackThread},
            error::WhipWhepServerError,
            negotiated_codecs::{
                WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping, audio_codec_negotiated,
            },
            video_input_processing_loop::{VideoInputLoop, VideoTrackThread},
        },
    },
    thread_utils::InitializableThread,
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

    let handle = AudioTrackThread::spawn(
        format!("WHIP input audio, endpoint_id: {}", endpoint_id),
        (ctx.clone(), samples_sender),
    )?;

    let audio_input_loop = AudioInputLoop {
        sync_point,
        track,
        rtc_receiver,
        handle,
    };

    audio_input_loop.run(ctx).await?;

    Ok(())
}

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

    let handle = VideoTrackThread::spawn(
        format!("WHIP input video, endpoint_id: {}", endpoint_id),
        (
            ctx.clone(),
            decoder_mapping,
            payload_type_mapping,
            frame_sender,
        ),
    )?;

    let video_input_loop = VideoInputLoop {
        sync_point,
        track,
        rtc_receiver,
        handle,
    };

    video_input_loop.run(ctx).await?;

    Ok(())
}
