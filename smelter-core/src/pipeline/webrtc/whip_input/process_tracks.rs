use std::sync::Arc;

use crossbeam_channel::bounded;
use tracing::warn;
use webrtc::{
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
    rtp_transceiver::RTCRtpTransceiver, track::track_remote::TrackRemote,
};

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::{
        decoder::VideoDecoderMapping, rtp::{depayloader::VideoPayloadTypeMapping, RtpNtpSyncPoint}, utils::input_buffer::InputBuffer, webrtc::{
            audio_input_processing_loop::{AudioInputLoop, AudioTrackThread}, error::WhipWhepServerError, negotiated_codecs::{
                audio_codec_negotiated, WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping
            }, video_input_processing_loop::{VideoInputLoop, VideoTrackThread}, WhipWhepServerState
        }
    },
    thread_utils::InitializableThread,
};

pub async fn process_audio_track(
    sync_point: Arc<RtpNtpSyncPoint>,
    buffer: InputBuffer,
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
        format!("WHIP input audio, endpoint_id: {endpoint_id}"),
        (ctx.clone(), samples_sender),
    )?;

    let audio_input_loop = AudioInputLoop {
        sync_point,
        track,
        rtc_receiver,
        handle,
        buffer,
    };

    audio_input_loop.run(ctx).await?;

    Ok(())
}

pub async fn process_video_track(
    sync_point: Arc<RtpNtpSyncPoint>,
    buffer: InputBuffer,
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

    let (keyframe_request_sender, keyframe_request_receiver) = bounded(1);
    let ssrc = track.ssrc();
    let rtc_receiver_clone = rtc_receiver.clone();
    tokio::spawn(async move {
        let transport = rtc_receiver_clone.transport();
        for _ in keyframe_request_receiver.into_iter() {
            warn!("Sending PLI");
            let pli = PictureLossIndication {
                // For receive-only endpoints RTP sender SSRC can be set to 0.
                sender_ssrc: 0,
                media_ssrc: ssrc,
            };

            if let Err(err) = transport.write_rtcp(&[Box::new(pli)]).await {
                warn!(?err)
            }
        }
    });

    let handle = VideoTrackThread::spawn(
        format!("WHIP input video, endpoint_id: {endpoint_id}"),
        (
            ctx.clone(),
            decoder_mapping,
            payload_type_mapping,
            frame_sender,
            keyframe_request_sender,
        ),
    )?;

    let video_input_loop = VideoInputLoop {
        sync_point,
        track,
        rtc_receiver,
        handle,
        buffer,
    };

    video_input_loop.run(ctx).await?;

    Ok(())
}
