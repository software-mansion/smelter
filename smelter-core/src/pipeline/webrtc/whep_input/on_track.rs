use crossbeam_channel::Sender;
use smelter_render::{Frame, InputId};
use tracing::{Instrument, debug, info_span, trace, warn};
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;

use crate::{
    pipeline::{
        decoder::VideoDecoderMapping,
        rtp::{RtpTimestampSync, depayloader::VideoPayloadTypeMapping},
        webrtc::{
            input_rtcp_listener::RtcpListeners,
            input_rtp_reader::WebrtcRtpReader,
            input_thread::{AudioTrackThread, VideoTrackThread, start_pli_sender_task},
            negotiated_codecs::{
                WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping, audio_codec_negotiated,
            },
            whep_input::WhepTrackContext,
        },
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub fn handle_on_track(
    ctx: WhepTrackContext,
    input_id: InputId,
    input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    video_preferences: Vec<VideoDecoderOptions>,
) {
    let kind = ctx.track.kind();
    let span = info_span!("WHEP input track", ?kind, ?input_id);

    tokio::spawn(
        async move {
            debug!("on_track called");
            let result = match kind {
                RTPCodecType::Audio => process_audio_track(ctx, input_samples_sender).await,
                RTPCodecType::Video => {
                    process_video_track(ctx, frame_sender, video_preferences).await
                }
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
    ctx: WhepTrackContext,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
) -> Result<(), WebrtcClientError> {
    if !audio_codec_negotiated(&ctx.rtc_receiver).await {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WebrtcClientError::NoAudioCodecNegotiated);
    };

    let handle = AudioTrackThread::spawn(
        "WHEP input audio",
        (ctx.pipeline_ctx.clone(), samples_sender),
    )?;

    let mut rtp_reader = WebrtcRtpReader {
        track: ctx.track,
        timestamp_sync: RtpTimestampSync::new(&ctx.sync_point, 48_000, ctx.buffer),
        rtcp_listeners: RtcpListeners::start(&ctx.pipeline_ctx, ctx.rtc_receiver),
        state: Default::default(),
    };

    while let Some(packet) = rtp_reader.read_packet().await {
        trace!(?packet, "Sending RTP packet");
        for packet in packet.into_iter() {
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
    }

    Ok(())
}

async fn process_video_track(
    ctx: WhepTrackContext,
    frame_sender: Sender<PipelineEvent<Frame>>,
    video_preferences: Vec<VideoDecoderOptions>,
) -> Result<(), WebrtcClientError> {
    let (Some(decoder_mapping), Some(payload_type_mapping)) = (
        VideoDecoderMapping::from_webrtc_receiver(&ctx.rtc_receiver, &video_preferences).await,
        VideoPayloadTypeMapping::from_webrtc_receiver(&ctx.rtc_receiver).await,
    ) else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WebrtcClientError::NoVideoCodecNegotiated);
    };

    let keyframe_request_sender = start_pli_sender_task(&ctx.track, &ctx.rtc_receiver);
    keyframe_request_sender.send();

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

    let mut rtp_reader = WebrtcRtpReader {
        track: ctx.track,
        timestamp_sync: RtpTimestampSync::new(&ctx.sync_point, 90_000, ctx.buffer),
        rtcp_listeners: RtcpListeners::start(&ctx.pipeline_ctx, ctx.rtc_receiver),
        state: Default::default(),
    };

    while let Some(packet) = rtp_reader.read_packet().await {
        for packet in packet.into_iter() {
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
    }
    Ok(())
}
