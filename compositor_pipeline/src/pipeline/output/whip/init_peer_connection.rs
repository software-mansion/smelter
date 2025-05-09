use crate::{
    audio_mixer::AudioChannels,
    pipeline::encoder::{AudioEncoderOptions, VideoEncoderOptions},
};

use super::{WhipCtx, WhipError};
use std::sync::Arc;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8},
        APIBuilder,
    },
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    peer_connection::{configuration::RTCConfiguration, RTCPeerConnection},
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType},
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
        RTCPFeedback, RTCRtpTransceiver, RTCRtpTransceiverInit,
    },
};

pub async fn init_peer_connection(
    whip_ctx: &WhipCtx,
) -> Result<
    (
        Arc<RTCPeerConnection>,
        Option<Arc<RTCRtpTransceiver>>,
        Option<Arc<RTCRtpTransceiver>>,
    ),
    WhipError,
> {
    let mut media_engine = MediaEngine::default();

    register_codecs(&mut media_engine, whip_ctx)?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: whip_ctx.pipeline_ctx.stun_servers.to_vec(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    let video_transceiver = if whip_ctx.options.video.is_some() {
        Some(
            peer_connection
                .add_transceiver_from_kind(
                    RTPCodecType::Video,
                    Some(RTCRtpTransceiverInit {
                        direction: RTCRtpTransceiverDirection::Sendonly,
                        send_encodings: vec![],
                    }),
                )
                .await
                .map_err(WhipError::PeerConnectionInitError)?,
        )
    } else {
        None
    };
    let audio_transceiver = if whip_ctx.options.audio.is_some() {
        Some(
            peer_connection
                .add_transceiver_from_kind(
                    RTPCodecType::Audio,
                    Some(RTCRtpTransceiverInit {
                        direction: RTCRtpTransceiverDirection::Sendonly,
                        send_encodings: vec![],
                    }),
                )
                .await
                .map_err(WhipError::PeerConnectionInitError)?,
        )
    } else {
        None
    };

    Ok((peer_connection, video_transceiver, audio_transceiver))
}

fn register_codecs(
    media_engine: &mut MediaEngine,
    whip_ctx: &WhipCtx,
) -> webrtc::error::Result<()> {
    let video_encoder_preferences = whip_ctx
        .options
        .video
        .as_ref()
        .map(|v| v.encoder_preferences.clone());
    let audio_encoder_preferences = whip_ctx
        .options
        .audio
        .as_ref()
        .map(|a| a.encoder_preferences.clone());

    for encoder_options in &audio_encoder_preferences.unwrap_or_default() {
        if let AudioEncoderOptions::Opus(opts) = encoder_options {
            let channels = match opts.channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            };
            media_engine.register_codec(
                RTCRtpCodecParameters {
                    capability: RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_OPUS.to_owned(),
                        clock_rate: opts.sample_rate,
                        channels,
                        sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                        rtcp_feedback: vec![],
                    },
                    payload_type: 111,
                    ..Default::default()
                },
                RTPCodecType::Audio,
            )?;
        }
    }

    let video_rtcp_feedback = vec![
        RTCPFeedback {
            typ: "goog-remb".to_owned(),
            parameter: "".to_owned(),
        },
        RTCPFeedback {
            typ: "ccm".to_owned(),
            parameter: "fir".to_owned(),
        },
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "".to_owned(),
        },
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "pli".to_owned(),
        },
    ];

    for encoder_options in &video_encoder_preferences.unwrap_or_default() {
        match encoder_options {
            VideoEncoderOptions::H264(_) => {
                let h264_codec_parameters = vec![
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_H264.to_owned(),
                            clock_rate: 90000,
                            channels: 0,
                            sdp_fmtp_line:
                                "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f"
                                    .to_owned(),
                            rtcp_feedback: video_rtcp_feedback.clone(),
                        },
                        payload_type: 102,
                        ..Default::default()
                    },
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_H264.to_owned(),
                            clock_rate: 90000,
                            channels: 0,
                            sdp_fmtp_line:
                                "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f"
                                    .to_owned(),
                            rtcp_feedback: video_rtcp_feedback.clone(),
                        },
                        payload_type: 127,
                        ..Default::default()
                    },
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_H264.to_owned(),
                            clock_rate: 90000,
                            channels: 0,
                            sdp_fmtp_line:
                                "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                                    .to_owned(),
                            rtcp_feedback: video_rtcp_feedback.clone(),
                        },
                        payload_type: 125,
                        ..Default::default()
                    },
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_H264.to_owned(),
                            clock_rate: 90000,
                            channels: 0,
                            sdp_fmtp_line:
                                "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f"
                                    .to_owned(),
                            rtcp_feedback: video_rtcp_feedback.clone(),
                        },
                        payload_type: 108,
                        ..Default::default()
                    },
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_H264.to_owned(),
                            clock_rate: 90000,
                            channels: 0,
                            sdp_fmtp_line:
                                "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032"
                                    .to_owned(),
                            rtcp_feedback: video_rtcp_feedback.clone(),
                        },
                        payload_type: 123,
                        ..Default::default()
                    },
                ];
                for codec in h264_codec_parameters {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoEncoderOptions::VP8(_) => {
                media_engine.register_codec(
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_VP8.to_owned(),
                            clock_rate: 90000,
                            channels: 0,
                            sdp_fmtp_line: "".to_owned(),
                            rtcp_feedback: video_rtcp_feedback.clone(),
                        },
                        payload_type: 96,
                        ..Default::default()
                    },
                    RTPCodecType::Video,
                )?;
            }
        }
    }

    Ok(())
}
