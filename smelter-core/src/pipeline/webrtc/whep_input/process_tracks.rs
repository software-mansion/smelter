use std::sync::Arc;

use crossbeam_channel::Sender;
use smelter_render::Frame;
use tracing::{Instrument, Level, debug, span, warn};
use webrtc::{
    rtcp::{packet::Packet, payload_feedbacks::picture_loss_indication::PictureLossIndication},
    rtp_transceiver::{RTCRtpTransceiver, rtp_codec::RTPCodecType, rtp_receiver::RTCRtpReceiver},
    track::track_remote::TrackRemote,
};

use crate::{
    PipelineCtx, PipelineEvent,
    codecs::VideoDecoderOptions,
    pipeline::{
        decoder::VideoDecoderMapping,
        rtp::{RtpNtpSyncPoint, depayloader::VideoPayloadTypeMapping},
        webrtc::{
            audio_input_processing_loop::{AudioInputLoop, AudioTrackThread},
            negotiated_codecs::{
                WebrtcVideoDecoderMapping, WebrtcVideoPayloadTypeMapping, audio_codec_negotiated,
            },
            peer_connection_recvonly::RecvonlyPeerConnection,
            video_input_processing_loop::{VideoInputLoop, VideoTrackThread},
        },
    },
    prelude::{InputAudioSamples, WebrtcClientError},
    thread_utils::InitializableThread,
};

pub fn setup_track_processing(
    pc: &RecvonlyPeerConnection,
    ctx: &Arc<PipelineCtx>,
    input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    video_preferences: Vec<VideoDecoderOptions>,
) {
    let ctx = ctx.clone();
    let sync_point = RtpNtpSyncPoint::new(ctx.queue_sync_point);
    pc.on_track(Box::new(move |track, _, transceiver| {
        debug!(
            kind=?track.kind(),
            "on_track called"
        );

        let span = span!(Level::INFO, "WHEP input track", track_type=?track.kind());

        match track.kind() {
            RTPCodecType::Audio => {
                tokio::spawn(
                    process_audio_track(
                        ctx.clone(),
                        sync_point.clone(),
                        input_samples_sender.clone(),
                        track,
                        transceiver,
                    )
                    .instrument(span),
                );
            }
            RTPCodecType::Video => {
                tokio::spawn(
                    process_video_track(
                        ctx.clone(),
                        sync_point.clone(),
                        frame_sender.clone(),
                        track,
                        transceiver,
                        video_preferences.clone(),
                    )
                    .instrument(span),
                );
            }
            RTPCodecType::Unspecified => {
                warn!("Unknown track kind")
            }
        }

        Box::pin(async {})
    }))
}

async fn process_audio_track(
    ctx: Arc<PipelineCtx>,
    sync_point: Arc<RtpNtpSyncPoint>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    track: Arc<TrackRemote>,
    transceiver: Arc<RTCRtpTransceiver>,
) -> Result<(), WebrtcClientError> {
    let rtc_receiver = transceiver.receiver().await;
    if !audio_codec_negotiated(rtc_receiver.clone()).await {
        warn!("Skipping audio track, no valid codec negotiated");
        return Err(WebrtcClientError::NoAudioCodecNegotiated);
    };

    let handle = AudioTrackThread::spawn("WHEP input audio", (ctx.clone(), samples_sender))?;

    let audio_input_loop = AudioInputLoop {
        sync_point,
        track,
        rtc_receiver,
        handle,
    };

    audio_input_loop.run(ctx).await?;

    Ok(())
}

async fn process_video_track(
    ctx: Arc<PipelineCtx>,
    sync_point: Arc<RtpNtpSyncPoint>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    track: Arc<TrackRemote>,
    transceiver: Arc<RTCRtpTransceiver>,
    video_preferences: Vec<VideoDecoderOptions>,
) -> Result<(), WebrtcClientError> {
    let rtc_receiver = transceiver.receiver().await;
    let (Some(decoder_mapping), Some(payload_type_mapping)) = (
        VideoDecoderMapping::from_webrtc_transceiver(transceiver.clone(), &video_preferences).await,
        VideoPayloadTypeMapping::from_webrtc_transceiver(transceiver).await,
    ) else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WebrtcClientError::NoVideoCodecNegotiated);
    };

    request_keyframe(&track, &rtc_receiver).await?;
    let handle = VideoTrackThread::spawn(
        "WHEP input video",
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

    let rtcp_packets: Vec<Box<dyn Packet + Send + Sync>> = vec![Box::new(pli)];
    rtc_receiver.transport().write_rtcp(&rtcp_packets).await
}
