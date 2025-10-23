use std::sync::Arc;

use crossbeam_channel::Sender;
use smelter_render::{Frame, InputId};
use tracing::{Instrument, debug, info_span, warn};
use webrtc::{
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
    rtp_transceiver::{rtp_codec::RTPCodecType, rtp_receiver::RTCRtpReceiver},
    track::track_remote::TrackRemote,
};

use crate::{
    pipeline::{
        decoder::VideoDecoderMapping,
        rtp::depayloader::VideoPayloadTypeMapping,
        webrtc::{
            audio_input_processing_loop::{AudioInputLoop, AudioTrackThread},
            negotiated_codecs::{
                WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping, audio_codec_negotiated,
            },
            video_input_processing_loop::{VideoInputLoop, VideoTrackThread},
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
    let _span = info_span!("WHEP input track", ?kind, ?input_id).entered();
    debug!("on_track called");

    match kind {
        RTPCodecType::Audio => {
            tokio::spawn(
                process_audio_track(ctx, input_samples_sender).instrument(tracing::Span::current()),
            );
        }
        RTPCodecType::Video => {
            tokio::spawn(
                process_video_track(ctx, frame_sender, video_preferences)
                    .instrument(tracing::Span::current()),
            );
        }
        RTPCodecType::Unspecified => {
            warn!("Unknown track kind")
        }
    }
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

    let audio_input_loop = AudioInputLoop {
        handle,
        sync_point: ctx.sync_point,
        buffer: ctx.buffer,
        track: ctx.track,
        rtc_receiver: ctx.rtc_receiver,
    };

    audio_input_loop.run(&ctx.pipeline_ctx).await?;

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

    request_keyframe(&ctx.track, &ctx.rtc_receiver).await?;
    let handle = VideoTrackThread::spawn(
        "WHEP input video",
        (
            ctx.pipeline_ctx.clone(),
            decoder_mapping,
            payload_type_mapping,
            frame_sender,
        ),
    )?;

    let video_input_loop = VideoInputLoop {
        handle,
        sync_point: ctx.sync_point,
        buffer: ctx.buffer,
        track: ctx.track,
        rtc_receiver: ctx.rtc_receiver,
    };

    video_input_loop.run(&ctx.pipeline_ctx).await?;

    Ok(())
}

async fn request_keyframe(
    track: &Arc<TrackRemote>,
    rtc_receiver: &Arc<RTCRtpReceiver>,
) -> Result<usize, webrtc::Error> {
    let ssrc = track.ssrc();
    let pli = PictureLossIndication {
        // For receive-only endpoints RTP sender SSRC can be set to 0.
        sender_ssrc: 0,
        media_ssrc: ssrc,
    };

    rtc_receiver.transport().write_rtcp(&[Box::new(pli)]).await
}
