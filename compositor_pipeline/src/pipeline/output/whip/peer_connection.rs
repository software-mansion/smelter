use std::sync::Arc;

use super::{WhipError, WhipSenderOptions};
use tracing::debug;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
        APIBuilder,
    },
    ice_transport::{
        ice_connection_state::RTCIceConnectionState, ice_gatherer::OnLocalCandidateHdlrFn,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription,
        RTCPeerConnection,
    },
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType},
        rtp_sender::RTCRtpSender,
        RTCPFeedback,
    },
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::{
    audio_mixer::AudioChannels,
    pipeline::{
        encoder::{AudioEncoderOptions, VideoEncoderOptions},
        PipelineCtx,
    },
};

#[derive(Debug, Clone)]
pub(super) struct PeerConnection {
    pc: Arc<RTCPeerConnection>,
}

impl PeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        options: &WhipSenderOptions,
    ) -> Result<Self, WhipError> {
        let mut media_engine = media_engine_with_codecs(options)?;
        let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();

        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: ctx.stun_servers.to_vec(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let peer_connection = Arc::new(api.new_peer_connection(config).await?);

        peer_connection.on_ice_connection_state_change(Box::new(
            move |connection_state: RTCIceConnectionState| {
                debug!("Connection state has changed {connection_state}.");
                Box::pin(async {})
            },
        ));

        Ok(Self {
            pc: peer_connection,
        })
    }

    pub async fn new_video_track(&self) -> Result<Arc<RTCRtpSender>, WhipError> {
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            "video".to_string(),
            "webrtc".to_string(),
        ));
        let sender = self
            .pc
            .add_track(track)
            .await
            .map_err(WhipError::PeerConnectionInitError)?;
        Ok(sender)
    }

    pub async fn new_audio_track(&self) -> Result<Arc<RTCRtpSender>, WhipError> {
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            "audio".to_string(),
            "webrtc".to_string(),
        ));
        let sender = self
            .pc
            .add_track(track)
            .await
            .map_err(WhipError::PeerConnectionInitError)?;
        Ok(sender)
    }

    pub async fn set_remote_description(
        &self,
        answer: RTCSessionDescription,
    ) -> Result<(), WhipError> {
        self.pc
            .set_remote_description(answer)
            .await
            .map_err(WhipError::RemoteDescriptionError)
    }

    pub async fn set_local_description(
        &self,
        offer: RTCSessionDescription,
    ) -> Result<(), WhipError> {
        self.pc
            .set_local_description(offer)
            .await
            .map_err(WhipError::LocalDescriptionError)
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription, WhipError> {
        self.pc
            .create_offer(None)
            .await
            .map_err(WhipError::OfferCreationError)
    }

    pub fn on_ice_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.pc.on_ice_candidate(f);
    }
}

fn media_engine_with_codecs(options: &WhipSenderOptions) -> webrtc::error::Result<MediaEngine> {
    let mut media_engine = MediaEngine::default();

    let video_encoder_preferences = options
        .video
        .as_ref()
        .map(|v| v.encoder_preferences.clone());
    let audio_encoder_preferences = options
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
            VideoEncoderOptions::Vp8(_) => {
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
            VideoEncoderOptions::Vp9(_) => {
                media_engine.register_codec(
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_VP9.to_owned(),
                            clock_rate: 90000,
                            channels: 0,
                            sdp_fmtp_line: "".to_owned(),
                            rtcp_feedback: video_rtcp_feedback.clone(),
                        },
                        payload_type: 98,
                        ..Default::default()
                    },
                    RTPCodecType::Video,
                )?;
            }
        }
    }

    Ok(media_engine)
}
