use tracing::{Instrument, debug, info_span, trace, warn};
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;

use crate::{
    pipeline::{
        decoder::VideoDecoderMapping,
        rtp::{RtpJitterBuffer, depayloader::VideoPayloadTypeMapping},
        webrtc::{
            input_rtp_reader::WebrtcRtpReader,
            input_thread::{AudioTrackThread, VideoTrackThread},
            negotiated_codecs::{
                WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping, audio_codec_negotiated,
            },
            whep_input::WhepTrackContext,
        },
    },
    queue::QueueSender,
    utils::InitializableThread,
};

use crate::prelude::*;

pub fn handle_on_track(
    ctx: WhepTrackContext,
    input_ref: Ref<InputId>,
    video_preferences: Vec<VideoDecoderOptions>,
    video_sender: &mut Option<QueueSender<Frame>>,
    audio_sender: &mut Option<QueueSender<InputAudioSamples>>,
) {
    let kind = ctx.track.kind();
    let span = info_span!("WHEP input track", ?kind, input_id=%input_ref);
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
            let pipeline_ctx = ctx.pipeline_ctx.clone();
            let task = async move {
                if let Err(err) = process_audio_track(ctx, input_ref, audio_sender).await {
                    // TODO: address after WhipWhepServerError rework
                    warn!(?err, "On track handler failed")
                }
            };
            pipeline_ctx.spawn_tracked(task.instrument(span));
        }
        RTPCodecType::Video => {
            let Some(video_sender) = video_sender.take() else {
                warn!("Audio track already started");
                return;
            };
            let pipeline_ctx = ctx.pipeline_ctx.clone();
            let task = async move {
                if let Err(err) =
                    process_video_track(ctx, input_ref, video_preferences, video_sender).await
                {
                    // TODO: address after WhipWhepServerError rework
                    warn!(?err, "On track handler failed")
                }
            };
            pipeline_ctx.spawn_tracked(task.instrument(span));
        }
        RTPCodecType::Unspecified => {
            warn!("Unknown track kind");
        }
    };
}

async fn process_audio_track(
    ctx: WhepTrackContext,
    input_ref: Ref<InputId>,
    samples_sender: QueueSender<InputAudioSamples>,
) -> Result<(), WebrtcClientError> {
    if !audio_codec_negotiated(&ctx.rtc_receiver).await {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WebrtcClientError::NoAudioCodecNegotiated);
    };

    let handle = AudioTrackThread::spawn(
        "WHEP input audio",
        (ctx.pipeline_ctx.clone(), samples_sender),
    )?;

    let stats_sender = ctx.pipeline_ctx.stats_sender.clone();
    let mut rtp_reader = WebrtcRtpReader::new(
        &ctx.pipeline_ctx,
        ctx.track,
        ctx.rtc_receiver,
        RtpJitterBuffer::new(
            ctx.buffer,
            48_000,
            Box::new(move |event| {
                stats_sender.send(WhepInputStatsEvent::AudioRtp(event).into_event(&input_ref));
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
    ctx: WhepTrackContext,
    input_ref: Ref<InputId>,
    video_preferences: Vec<VideoDecoderOptions>,
    frame_sender: QueueSender<Frame>,
) -> Result<(), WebrtcClientError> {
    let (Some(decoder_mapping), Some(payload_type_mapping)) = (
        VideoDecoderMapping::from_webrtc_receiver(&ctx.rtc_receiver, &video_preferences).await,
        VideoPayloadTypeMapping::from_webrtc_receiver(&ctx.rtc_receiver).await,
    ) else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WebrtcClientError::NoVideoCodecNegotiated);
    };

    let on_stats_event = {
        let stats_sender = ctx.pipeline_ctx.stats_sender.clone();
        let input_ref = input_ref.clone();
        Box::new(move |event| {
            stats_sender.send(WhepInputStatsEvent::VideoRtp(event).into_event(&input_ref));
        })
    };
    let mut rtp_reader = WebrtcRtpReader::new(
        &ctx.pipeline_ctx,
        ctx.track,
        ctx.rtc_receiver,
        RtpJitterBuffer::new(ctx.buffer, 90_000, on_stats_event),
    );
    let keyframe_request_sender = rtp_reader.enable_pli().await;

    let handle = VideoTrackThread::spawn(
        "WHEP input video",
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
