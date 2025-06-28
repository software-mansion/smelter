use crossbeam_channel::Sender;
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{rtp_codec::RTCRtpCodecCapability, rtp_sender::RTCRtpSender},
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::pipeline::{
    encoder::{
        ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder, ffmpeg_vp9::FfmpegVp9Encoder,
        opus::OpusEncoder, AudioEncoderOptions, VideoEncoderOptions,
    },
    output::{
        rtp::payloader::{PayloadedCodec, PayloaderOptions},
        whip::track_task_audio::spawn_audio_track_thread,
    },
};

use super::{
    track_task_audio::WhipAudioTrackThreadHandle,
    track_task_video::{spawn_video_track_thread, WhipVideoTrackThreadHandle},
    WhipCtx, WhipError,
};

pub trait MatchCodecCapability {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool;
}

impl MatchCodecCapability for VideoEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            VideoEncoderOptions::H264(_) => capability.mime_type == MIME_TYPE_H264,
            VideoEncoderOptions::VP8(_) => capability.mime_type == MIME_TYPE_VP8,
            VideoEncoderOptions::VP9(_) => capability.mime_type == MIME_TYPE_VP9,
        }
    }
}

impl MatchCodecCapability for AudioEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            AudioEncoderOptions::Opus(_) => capability.mime_type == MIME_TYPE_OPUS,
            AudioEncoderOptions::Aac(_) => false,
        }
    }
}

pub async fn setup_video_track(
    whip_ctx: &Arc<WhipCtx>,
    rtc_sender: Arc<RTCRtpSender>,
    encoder_preferences: Vec<VideoEncoderOptions>,
) -> Result<
    (
        WhipVideoTrackThreadHandle,
        (
            mpsc::Receiver<(rtp::packet::Packet, Duration)>,
            Arc<TrackLocalStaticRTP>,
        ),
    ),
    WhipError,
> {
    let rtc_sender_params = rtc_sender.get_parameters().await;
    debug!("RTCRtpSender video params: {:#?}", rtc_sender_params);
    let supported_codecs = &rtc_sender_params.rtp_parameters.codecs;

    let Some((options, codec_params)) = encoder_preferences.iter().find_map(|encoder_options| {
        let supported = supported_codecs.iter().find_map(|codec_params| {
            match encoder_options.matches(&codec_params.capability) {
                true => Some(codec_params.clone()),
                false => None,
            }
        })?;
        Some((encoder_options.clone(), supported.clone()))
    }) else {
        return Err(WhipError::NoVideoCodecNegotiated);
    };

    let track = Arc::new(TrackLocalStaticRTP::new(
        codec_params.capability.clone(),
        "video".to_string(),
        "webrtc-rs".to_string(),
    ));

    rtc_sender.replace_track(Some(track.clone())).await?;

    fn payloader_options(codec: PayloadedCodec, payload_type: u8, ssrc: u32) -> PayloaderOptions {
        PayloaderOptions {
            codec,
            payload_type,
            clock_rate: 90_000,
            mtu: 1200,
            ssrc,
        }
    }

    let ssrc = rtc_sender_params.encodings.first().unwrap().ssrc;
    let (sender, receiver) = mpsc::channel(10);
    let handle = match options {
        VideoEncoderOptions::H264(options) => spawn_video_track_thread::<FfmpegH264Encoder>(
            whip_ctx.pipeline_ctx.clone(),
            whip_ctx.output_id.clone(),
            options,
            payloader_options(PayloadedCodec::H264, codec_params.payload_type, ssrc),
            sender,
        ),
        VideoEncoderOptions::VP8(options) => spawn_video_track_thread::<FfmpegVp8Encoder>(
            whip_ctx.pipeline_ctx.clone(),
            whip_ctx.output_id.clone(),
            options,
            payloader_options(PayloadedCodec::Vp8, codec_params.payload_type, ssrc),
            sender,
        ),
        VideoEncoderOptions::VP9(options) => spawn_video_track_thread::<FfmpegVp9Encoder>(
            whip_ctx.pipeline_ctx.clone(),
            whip_ctx.output_id.clone(),
            options,
            payloader_options(PayloadedCodec::Vp9, codec_params.payload_type, ssrc),
            sender,
        ),
    }
    .unwrap();

    handle_keyframe_requests(
        whip_ctx.clone(),
        rtc_sender.clone(),
        handle.keyframe_request_sender.clone(),
    );

    Ok((handle, (receiver, track)))
}

pub async fn setup_audio_track(
    whip_ctx: &Arc<WhipCtx>,
    rtc_sender: Arc<RTCRtpSender>,
    encoder_preferences: Vec<AudioEncoderOptions>,
) -> Result<
    (
        WhipAudioTrackThreadHandle,
        (
            mpsc::Receiver<(rtp::packet::Packet, Duration)>,
            Arc<TrackLocalStaticRTP>,
        ),
    ),
    WhipError,
> {
    let rtc_sender_params = rtc_sender.get_parameters().await;
    debug!("RTCRtpSender audio params: {:#?}", rtc_sender_params);

    let supported_codecs = &rtc_sender_params.rtp_parameters.codecs;
    let Some((options, codec_params)) = encoder_preferences.iter().find_map(|encoder_options| {
        let supported = supported_codecs.iter().find_map(|codec_params| {
            match encoder_options.matches(&codec_params.capability) {
                true => Some(codec_params.clone()),
                false => None,
            }
        })?;
        Some((encoder_options.clone(), supported.clone()))
    }) else {
        return Err(WhipError::NoAudioCodecNegotiated);
    };

    let track = Arc::new(TrackLocalStaticRTP::new(
        codec_params.capability.clone(),
        "audio".to_string(),
        "webrtc-rs".to_string(),
    ));

    rtc_sender.replace_track(Some(track.clone())).await?;

    fn payloader_options(codec: PayloadedCodec, payload_type: u8, ssrc: u32) -> PayloaderOptions {
        PayloaderOptions {
            codec,
            payload_type,
            clock_rate: 48_000,
            mtu: 1200,
            ssrc,
        }
    }

    let ssrc = rtc_sender_params.encodings.first().unwrap().ssrc;
    let (sender, receiver) = mpsc::channel(10);
    let handle = match options {
        AudioEncoderOptions::Opus(options) => spawn_audio_track_thread::<OpusEncoder>(
            whip_ctx.pipeline_ctx.clone(),
            whip_ctx.output_id.clone(),
            options,
            payloader_options(PayloadedCodec::Opus, codec_params.payload_type, ssrc),
            sender,
        ),
        AudioEncoderOptions::Aac(_options) => return Err(WhipError::UnsupportedCodec("aac")),
    }
    .unwrap();

    Ok((handle, (receiver, track)))
}

fn handle_keyframe_requests(
    whip_ctx: Arc<WhipCtx>,
    sender: Arc<RTCRtpSender>,
    keyframe_sender: Sender<()>,
) {
    whip_ctx.pipeline_ctx.tokio_rt.spawn(async move {
        loop {
            if let Ok((packets, _)) = sender.read_rtcp().await {
                for packet in packets {
                    if packet
                        .as_any()
                        .downcast_ref::<PictureLossIndication>()
                        .is_some()
                    {
                        info!("Request keyframe");
                        if let Err(err) = keyframe_sender.send(()) {
                            warn!(%err, "Failed to send keyframe request to the encoder.");
                        };
                    }
                }
            } else {
                debug!("Failed to read RTCP packets from the sender.");
            }
        }
    });
}
