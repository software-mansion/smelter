use tracing::{Instrument, debug, info_span, trace, warn};
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;

use crate::{
    PipelineEvent,
    codecs::VideoDecoderOptions,
    pipeline::{
        decoder::VideoDecoderMapping,
        rtp::{RtpJitterBuffer, depayloader::VideoPayloadTypeMapping},
        webrtc::{
            error::WhipWhepServerError,
            input_rtp_reader::WebrtcRtpReader,
            input_thread::{AudioTrackThread, VideoTrackThread},
            negotiated_codecs::{
                WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping, audio_codec_negotiated,
            },
            whip_input::WhipTrackContext,
        },
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub(super) fn handle_on_track(
    ctx: WhipTrackContext,
    input_ref: Ref<InputId>,
    video_preferences: Vec<VideoDecoderOptions>,
) {
    let kind = ctx.track.kind();
    let span = info_span!("WHIP input track", ?kind, input_id=%input_ref);

    tokio::spawn(
        async move {
            debug!("on_track called");
            let result = match kind {
                RTPCodecType::Audio => process_audio_track(ctx, input_ref).await,
                RTPCodecType::Video => process_video_track(ctx, input_ref, video_preferences).await,
                RTPCodecType::Unspecified => {
                    warn!("Unknown track kind");
                    Ok(())
                }
            };
            if let Err(err) = result {
                // TODO: address after WhipWhepServerError rework
                warn!(?err, "On track handler failed")
            }
        }
        .instrument(span),
    );
}

async fn process_audio_track(
    ctx: WhipTrackContext,
    input_ref: Ref<InputId>,
) -> Result<(), WhipWhepServerError> {
    if !audio_codec_negotiated(&ctx.rtc_receiver).await {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No audio codecs negotiated".to_string(),
        ));
    };

    let samples_sender = ctx
        .inputs
        .get_with(&input_ref, |input| Ok(input.input_samples_sender.clone()))?;

    let handle = AudioTrackThread::spawn(
        format!("WHIP input audio, input_id: {input_ref}"),
        (ctx.pipeline_ctx.clone(), samples_sender),
    )?;

    let stats_sender = ctx.pipeline_ctx.stats_sender.clone();
    let mut rtp_reader = WebrtcRtpReader::new(
        &ctx.pipeline_ctx,
        ctx.track,
        ctx.rtc_receiver,
        RtpJitterBuffer::new(
            &ctx.pipeline_ctx,
            ctx.buffer,
            48_000,
            Box::new(move |event| {
                stats_sender
                    .send_event(WhipInputStatsEvent::AudioRtp(event).into_event(&input_ref));
            }),
        ),
    );

    while let Some(packet) = rtp_reader.read_packet().await {
        trace!(?packet, "Sending RTP packet");
        if handle
            .rtp_packet_sender
            .send(PipelineEvent::Data(packet))
            .await
            .is_err()
        {
            debug!("Failed to send audio RTP packet, Channel closed.");
            break;
        }
    }

    Ok(())
}

async fn process_video_track(
    ctx: WhipTrackContext,
    input_ref: Ref<InputId>,
    video_preferences: Vec<VideoDecoderOptions>,
) -> Result<(), WhipWhepServerError> {
    let (Some(decoder_mapping), Some(payload_type_mapping)) = (
        VideoDecoderMapping::from_webrtc_receiver(&ctx.rtc_receiver, &video_preferences).await,
        VideoPayloadTypeMapping::from_webrtc_receiver(&ctx.rtc_receiver).await,
    ) else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No video codecs negotiated".to_string(),
        ));
    };

    let frame_sender = ctx
        .inputs
        .get_with(&input_ref, |input| Ok(input.frame_sender.clone()))?;

    let on_stats_event = {
        let stats_sender = ctx.pipeline_ctx.stats_sender.clone();
        let input_ref = input_ref.clone();
        Box::new(move |event| {
            stats_sender.send_event(WhipInputStatsEvent::VideoRtp(event).into_event(&input_ref));
        })
    };
    let mut rtp_reader = WebrtcRtpReader::new(
        &ctx.pipeline_ctx,
        ctx.track,
        ctx.rtc_receiver,
        RtpJitterBuffer::new(&ctx.pipeline_ctx, ctx.buffer, 90_000, on_stats_event),
    );
    let keyframe_request_sender = rtp_reader.enable_pli().await;

    let handle = VideoTrackThread::spawn(
        format!("WHIP input video, input_id: {input_ref}"),
        (
            ctx.pipeline_ctx.clone(),
            decoder_mapping,
            payload_type_mapping,
            frame_sender,
            keyframe_request_sender,
        ),
    )?;

    while let Some(packet) = rtp_reader.read_packet().await {
        trace!(?packet, "Sending RTP packet");
        if handle
            .rtp_packet_sender
            .send(PipelineEvent::Data(packet))
            .await
            .is_err()
        {
            debug!("Failed to send video RTP packet, Channel closed.");
            break;
        }
    }

    Ok(())
}
