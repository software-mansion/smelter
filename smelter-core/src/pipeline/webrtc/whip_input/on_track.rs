use smelter_render::InputId;
use tracing::{Instrument, debug, info_span, trace, warn};
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;

use crate::{
    PipelineEvent,
    codecs::VideoDecoderOptions,
    pipeline::{
        decoder::VideoDecoderMapping,
        rtp::{RtpJitterBuffer, RtpTimestampSync, depayloader::VideoPayloadTypeMapping},
        webrtc::{
            error::WhipWhepServerError,
            input_rtcp_listener::RtcpListeners,
            input_rtp_reader::WebrtcRtpReader,
            input_thread::{AudioTrackThread, VideoTrackThread, start_pli_sender_task},
            negotiated_codecs::{
                WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping, audio_codec_negotiated,
            },
            whip_input::WhipTrackContext,
        },
    },
    thread_utils::InitializableThread,
};

pub(super) fn handle_on_track(
    ctx: WhipTrackContext,
    input_id: InputId,
    video_preferences: Vec<VideoDecoderOptions>,
) {
    let kind = ctx.track.kind();
    let span = info_span!("WHIP input track", ?kind, ?input_id);

    tokio::spawn(
        async move {
            debug!("on_track called");
            let result = match kind {
                RTPCodecType::Audio => process_audio_track(ctx, input_id).await,
                RTPCodecType::Video => process_video_track(ctx, input_id, video_preferences).await,
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
    input_id: InputId,
) -> Result<(), WhipWhepServerError> {
    if !audio_codec_negotiated(&ctx.rtc_receiver).await {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No audio codecs negotiated".to_string(),
        ));
    };

    let samples_sender = ctx
        .inputs
        .get_with(&input_id, |input| Ok(input.input_samples_sender.clone()))?;

    let handle = AudioTrackThread::spawn(
        format!("WHIP input audio, endpoint_id: {input_id}"),
        (ctx.pipeline_ctx.clone(), samples_sender),
    )?;

    let mut rtp_reader = WebrtcRtpReader::new(
        ctx.track,
        RtcpListeners::start(&ctx.pipeline_ctx, ctx.rtc_receiver),
        RtpJitterBuffer::new(ctx.buffer, RtpTimestampSync::new(&ctx.sync_point, 48_000)),
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
    input_id: InputId,
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
        .get_with(&input_id, |input| Ok(input.frame_sender.clone()))?;
    let keyframe_request_sender = start_pli_sender_task(&ctx.track, &ctx.rtc_receiver);
    let handle = VideoTrackThread::spawn(
        format!("WHIP input video, endpoint_id: {input_id}"),
        (
            ctx.pipeline_ctx.clone(),
            decoder_mapping,
            payload_type_mapping,
            frame_sender,
            keyframe_request_sender,
        ),
    )?;

    let mut rtp_reader = WebrtcRtpReader::new(
        ctx.track,
        RtcpListeners::start(&ctx.pipeline_ctx, ctx.rtc_receiver),
        RtpJitterBuffer::new(ctx.buffer, RtpTimestampSync::new(&ctx.sync_point, 90_000)),
    );

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
