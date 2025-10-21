use std::sync::Arc;

use smelter_render::InputId;
use tracing::{Instrument, debug, info_span, warn};
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::{
        decoder::VideoDecoderMapping,
        rtp::{RtpNtpSyncPoint, depayloader::VideoPayloadTypeMapping},
        utils::input_buffer::InputBuffer,
        webrtc::{
            WhipWhepServerState,
            audio_input_processing_loop::{AudioInputLoop, AudioTrackThread},
            error::WhipWhepServerError,
            negotiated_codecs::{
                WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping, audio_codec_negotiated,
            },
            peer_connection_recvonly::OnTrackContext,
            video_input_processing_loop::{VideoInputLoop, VideoTrackThread},
        },
    },
    thread_utils::InitializableThread,
};

pub(super) fn handle_on_track(
    track_ctx: OnTrackContext,
    state: WhipWhepServerState,
    input_id: InputId,
    sync_point: Arc<RtpNtpSyncPoint>,
    buffer: InputBuffer,
    video_preferences: Vec<VideoDecoderOptions>,
) {
    let _span =
        info_span!("WHIP input track", track_type=?track_ctx.track.kind(), ?input_id).entered();
    debug!("on_track called");

    match track_ctx.track.kind() {
        RTPCodecType::Audio => {
            tokio::spawn(
                process_audio_track(track_ctx, sync_point, buffer, state, input_id)
                    .instrument(tracing::Span::current()),
            );
        }
        RTPCodecType::Video => {
            tokio::spawn(
                process_video_track(
                    track_ctx,
                    sync_point,
                    buffer,
                    state,
                    input_id,
                    video_preferences,
                )
                .instrument(tracing::Span::current()),
            );
        }
        RTPCodecType::Unspecified => {
            warn!("Unknown track kind")
        }
    }
}

async fn process_audio_track(
    track_ctx: OnTrackContext,
    sync_point: Arc<RtpNtpSyncPoint>,
    buffer: InputBuffer,
    state: WhipWhepServerState,
    input_id: InputId,
) -> Result<(), WhipWhepServerError> {
    if !audio_codec_negotiated(&track_ctx.rtc_receiver).await {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No audio codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx, .. } = state;
    let samples_sender =
        inputs.get_with(&input_id, |input| Ok(input.input_samples_sender.clone()))?;

    let handle = AudioTrackThread::spawn(
        format!("WHIP input audio, endpoint_id: {input_id}"),
        (ctx.clone(), samples_sender),
    )?;

    let audio_input_loop = AudioInputLoop {
        sync_point,
        handle,
        buffer,
        track_ctx,
    };

    audio_input_loop.run(ctx).await?;

    Ok(())
}

async fn process_video_track(
    track_ctx: OnTrackContext,
    sync_point: Arc<RtpNtpSyncPoint>,
    buffer: InputBuffer,
    state: WhipWhepServerState,
    input_id: InputId,
    video_preferences: Vec<VideoDecoderOptions>,
) -> Result<(), WhipWhepServerError> {
    let rtc_receiver = &track_ctx.rtc_receiver;
    let (Some(decoder_mapping), Some(payload_type_mapping)) = (
        VideoDecoderMapping::from_webrtc_receiver(rtc_receiver, &video_preferences).await,
        VideoPayloadTypeMapping::from_webrtc_receiver(rtc_receiver).await,
    ) else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No video codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx, .. } = state;
    let frame_sender = inputs.get_with(&input_id, |input| Ok(input.frame_sender.clone()))?;

    let handle = VideoTrackThread::spawn(
        format!("WHIP input video, endpoint_id: {input_id}"),
        (
            ctx.clone(),
            decoder_mapping,
            payload_type_mapping,
            frame_sender,
        ),
    )?;

    let video_input_loop = VideoInputLoop {
        sync_point,
        handle,
        buffer,
        track_ctx,
    };

    video_input_loop.run(ctx).await?;

    Ok(())
}
