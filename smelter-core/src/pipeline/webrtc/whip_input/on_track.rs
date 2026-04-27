use tracing::{Instrument, debug, info_span, trace, warn};
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;

use crate::{
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
    queue::QueueSender,
    utils::InitializableThread,
};

use crate::prelude::*;

pub(super) fn handle_on_track(
    ctx: WhipTrackContext,
    input_ref: Ref<InputId>,
    video_preferences: Vec<VideoDecoderOptions>,
    video_sender: &mut Option<QueueSender<Frame>>,
    audio_sender: &mut Option<QueueSender<InputAudioSamples>>,
) {
    let kind = ctx.track.kind();
    let span = info_span!("WHIP input track", ?kind, input_id=%input_ref);
    {
        let _span = span.enter();
        debug!("on_track called");
    }
    match kind {
        RTPCodecType::Audio => {
            let Some(audio_sender) = audio_sender.take() else {
                warn!("Audio track already started");
                return;
            };
            let task = async move {
                if let Err(err) = process_audio_track(ctx, input_ref, audio_sender).await {
                    // TODO: address after WhipWhepServerError rework
                    warn!(?err, "On track handler failed")
                }
            };
            tokio::spawn(task.instrument(span));
        }
        RTPCodecType::Video => {
            let Some(video_sender) = video_sender.take() else {
                warn!("Audio track already started");
                return;
            };
            let task = async move {
                if let Err(err) =
                    process_video_track(ctx, input_ref, video_preferences, video_sender).await
                {
                    // TODO: address after WhipWhepServerError rework
                    warn!(?err, "On track handler failed")
                }
            };
            tokio::spawn(task.instrument(span));
        }
        RTPCodecType::Unspecified => {
            warn!("Unknown track kind");
        }
    };
}

async fn process_audio_track(
    ctx: WhipTrackContext,
    input_ref: Ref<InputId>,
    samples_sender: QueueSender<InputAudioSamples>,
) -> Result<(), WhipWhepServerError> {
    if !audio_codec_negotiated(&ctx.rtc_receiver).await {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No audio codecs negotiated".to_string(),
        ));
    };

    let (handle, thread) = AudioTrackThread::spawn(
        format!("WHIP input audio, input_id: {input_ref}"),
        (ctx.pipeline_ctx.clone(), samples_sender),
    )?;

    let stats_sender = ctx.pipeline_ctx.stats_sender.clone();
    let mut rtp_reader = WebrtcRtpReader::new(
        &ctx.pipeline_ctx,
        ctx.track,
        ctx.rtc_receiver,
        RtpJitterBuffer::new(
            ctx.jitter_buffer_ctx,
            48_000,
            Box::new(move |event| {
                stats_sender.send(WhipInputStatsEvent::AudioRtp(event).into_event(&input_ref));
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

    // Close the channel explicitly, then join the worker thread so any
    // `Arc<vk_video::*>` it holds is released before this tokio task ends.
    drop(handle);
    if let Err(err) = thread.join() {
        warn!(?err, "WHIP audio track thread panicked during join");
    }

    Ok(())
}

async fn process_video_track(
    ctx: WhipTrackContext,
    input_ref: Ref<InputId>,
    video_preferences: Vec<VideoDecoderOptions>,
    video_sender: QueueSender<Frame>,
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

    let on_stats_event = {
        let stats_sender = ctx.pipeline_ctx.stats_sender.clone();
        let input_ref = input_ref.clone();
        Box::new(move |event| {
            stats_sender.send(WhipInputStatsEvent::VideoRtp(event).into_event(&input_ref));
        })
    };
    let mut rtp_reader = WebrtcRtpReader::new(
        &ctx.pipeline_ctx,
        ctx.track,
        ctx.rtc_receiver,
        RtpJitterBuffer::new(ctx.jitter_buffer_ctx, 90_000, on_stats_event),
    );
    let keyframe_request_sender = rtp_reader.enable_pli().await;

    let (handle, thread) = VideoTrackThread::spawn(
        format!("WHIP input video, input_id: {input_ref}"),
        (
            ctx.pipeline_ctx.clone(),
            decoder_mapping,
            payload_type_mapping,
            video_sender,
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

    drop(handle);
    if let Err(err) = thread.join() {
        warn!(?err, "WHIP video track thread panicked during join");
    }

    Ok(())
}
