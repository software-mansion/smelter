use rand::Rng;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::debug;
use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{rtp_codec::RTCRtpCodecCapability, rtp_sender::RTCRtpSender},
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::{
    pipeline::{
        encoder::{
            ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder, libopus::OpusEncoder,
        },
        rtp::payloader::{PayloadedCodec, PayloaderOptions},
        webrtc::{
            handle_keyframe_requests::handle_keyframe_requests,
            handle_packet_loss_requests::handle_packet_loss_requests,
            whip_output::{
                track_task_audio::{WhipAudioTrackThread, WhipAudioTrackThreadOptions},
                track_task_video::{WhipVideoTrackThread, WhipVideoTrackThreadOptions},
                PeerConnection,
            },
        },
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

use super::{
    track_task_audio::WhipAudioTrackThreadHandle, track_task_video::WhipVideoTrackThreadHandle,
    WhipOutputError, WhipSenderTrack,
};

pub trait MatchCodecCapability {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool;
}

impl MatchCodecCapability for VideoEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            VideoEncoderOptions::FfmpegH264(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_H264.to_lowercase()
            }
            VideoEncoderOptions::FfmpegVp8(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_VP8.to_lowercase()
            }
            VideoEncoderOptions::FfmpegVp9(_) => {
                capability.mime_type.to_lowercase() == MIME_TYPE_VP9.to_lowercase()
            }
        }
    }
}

impl MatchCodecCapability for AudioEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            AudioEncoderOptions::Opus(opt) => {
                let codec_match =
                    capability.mime_type.to_lowercase() == MIME_TYPE_OPUS.to_lowercase();

                let line = &capability.sdp_fmtp_line;
                let fec_negotiated = line.contains("useinbandfec=1");
                let fec_match = fec_negotiated == opt.forward_error_correction;

                codec_match && fec_match
            }
            AudioEncoderOptions::FdkAac(_) => false,
        }
    }
}

pub async fn setup_video_track(
    ctx: &Arc<PipelineCtx>,
    output_id: &OutputId,
    rtc_sender: Arc<RTCRtpSender>,
    options: &VideoWhipOptions,
) -> Result<(WhipVideoTrackThreadHandle, WhipSenderTrack), WhipOutputError> {
    let rtc_sender_params = rtc_sender.get_parameters().await;
    debug!("RTCRtpSender video params: {:#?}", rtc_sender_params);
    let supported_codecs = &rtc_sender_params.rtp_parameters.codecs;

    let Some((options, codec_params)) =
        options
            .encoder_preferences
            .iter()
            .find_map(|encoder_options| {
                let supported = supported_codecs.iter().find_map(|codec_params| {
                    match encoder_options.matches(&codec_params.capability) {
                        true => Some(codec_params.clone()),
                        false => None,
                    }
                })?;
                Some((encoder_options.clone(), supported.clone()))
            })
    else {
        return Err(WhipOutputError::NoVideoCodecNegotiated);
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

    let ssrc = match rtc_sender_params.encodings.first() {
        Some(e) => e.ssrc,
        None => rand::thread_rng().gen::<u32>(),
    };
    let (sender, receiver) = mpsc::channel(1000);
    let handle = match options {
        VideoEncoderOptions::FfmpegH264(options) => {
            WhipVideoTrackThread::<FfmpegH264Encoder>::spawn(
                output_id.clone(),
                WhipVideoTrackThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options,
                    payloader_options: payloader_options(
                        PayloadedCodec::H264,
                        codec_params.payload_type,
                        ssrc,
                    ),
                    chunks_sender: sender,
                },
            )
        }
        VideoEncoderOptions::FfmpegVp8(options) => WhipVideoTrackThread::<FfmpegVp8Encoder>::spawn(
            output_id.clone(),
            WhipVideoTrackThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options,
                payloader_options: payloader_options(
                    PayloadedCodec::Vp8,
                    codec_params.payload_type,
                    ssrc,
                ),
                chunks_sender: sender,
            },
        ),
        VideoEncoderOptions::FfmpegVp9(options) => WhipVideoTrackThread::<FfmpegVp9Encoder>::spawn(
            output_id.clone(),
            WhipVideoTrackThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options,
                payloader_options: payloader_options(
                    PayloadedCodec::Vp9,
                    codec_params.payload_type,
                    ssrc,
                ),
                chunks_sender: sender,
            },
        ),
    }?;

    handle_keyframe_requests(
        ctx,
        rtc_sender.clone(),
        handle.keyframe_request_sender.clone(),
    );

    Ok((handle, WhipSenderTrack { receiver, track }))
}

pub async fn setup_audio_track(
    ctx: &Arc<PipelineCtx>,
    output_id: &OutputId,
    rtc_sender: Arc<RTCRtpSender>,
    pc: PeerConnection,
    options: &AudioWhipOptions,
) -> Result<(WhipAudioTrackThreadHandle, WhipSenderTrack), WhipOutputError> {
    let rtc_sender_params = rtc_sender.get_parameters().await;
    debug!("RTCRtpSender audio params: {:#?}", rtc_sender_params);

    let supported_codecs = &rtc_sender_params.rtp_parameters.codecs;
    let Some((options, codec_params)) =
        options
            .encoder_preferences
            .iter()
            .find_map(|encoder_options| {
                let supported = supported_codecs.iter().find_map(|codec_params| {
                    match encoder_options.matches(&codec_params.capability) {
                        true => Some(codec_params.clone()),
                        false => None,
                    }
                })?;
                Some((encoder_options.clone(), supported.clone()))
            })
    else {
        return Err(WhipOutputError::NoAudioCodecNegotiated);
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

    let ssrc = match rtc_sender_params.encodings.first() {
        Some(e) => e.ssrc,
        None => rand::thread_rng().gen::<u32>(),
    };
    let (sender, receiver) = mpsc::channel(1000);
    let handle = match options {
        AudioEncoderOptions::Opus(options) => WhipAudioTrackThread::<OpusEncoder>::spawn(
            output_id.clone(),
            WhipAudioTrackThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options,
                payloader_options: payloader_options(
                    PayloadedCodec::Opus,
                    codec_params.payload_type,
                    ssrc,
                ),
                chunks_sender: sender,
            },
        ),
        AudioEncoderOptions::FdkAac(_options) => {
            return Err(WhipOutputError::UnsupportedCodec("aac"))
        }
    }?;

    handle_packet_loss_requests(
        ctx,
        pc.get_rtc_peer_connection(),
        rtc_sender.clone(),
        handle.packet_loss_sender.clone(),
        ssrc,
    );

    Ok((handle, WhipSenderTrack { receiver, track }))
}
