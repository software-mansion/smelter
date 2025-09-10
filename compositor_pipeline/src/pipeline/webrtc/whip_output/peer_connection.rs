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
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
        RTCPFeedback, RTCRtpTransceiverInit,
    },
    stats::StatsReport,
};

use std::sync::Arc;

use crate::prelude::*;

#[derive(Debug, Clone)]
pub(super) struct PeerConnection {
    pc: Arc<RTCPeerConnection>,
}

impl PeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        video_preferences: &Vec<VideoEncoderOptions>,
        audio_preferences: &[AudioEncoderOptions],
    ) -> Result<Self, WhipOutputError> {
        let mut media_engine = media_engine_with_codecs(video_preferences, audio_preferences)?;
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

    pub async fn new_video_track(&self) -> Result<Arc<RTCRtpSender>, WhipOutputError> {
        let transceiver = self
            .pc
            .add_transceiver_from_kind(
                RTPCodecType::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    send_encodings: vec![],
                }),
            )
            .await
            .map_err(WhipOutputError::PeerConnectionInitError)?;
        let sender = transceiver.sender().await;
        let rtc_sender_params = sender.get_parameters().await;
        debug!("RTCRtpSender video params: {:#?}", rtc_sender_params);
        Ok(sender)
    }

    pub async fn new_audio_track(&self) -> Result<Arc<RTCRtpSender>, WhipOutputError> {
        let transceiver = self
            .pc
            .add_transceiver_from_kind(
                RTPCodecType::Audio,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    send_encodings: vec![],
                }),
            )
            .await
            .map_err(WhipOutputError::PeerConnectionInitError)?;
        let sender = transceiver.sender().await;
        let rtc_sender_params = sender.get_parameters().await;
        debug!("RTCRtpSender audio params: {:#?}", rtc_sender_params);
        Ok(sender)
    }

    pub async fn set_remote_description(
        &self,
        answer: RTCSessionDescription,
    ) -> Result<(), WhipOutputError> {
        self.pc
            .set_remote_description(answer)
            .await
            .map_err(WhipOutputError::RemoteDescriptionError)
    }

    pub async fn set_local_description(
        &self,
        offer: RTCSessionDescription,
    ) -> Result<(), WhipOutputError> {
        self.pc
            .set_local_description(offer)
            .await
            .map_err(WhipOutputError::LocalDescriptionError)
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription, WhipOutputError> {
        self.pc
            .create_offer(None)
            .await
            .map_err(WhipOutputError::OfferCreationError)
    }

    pub fn on_ice_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.pc.on_ice_candidate(f);
    }

    pub async fn get_stats(&self) -> StatsReport {
        self.pc.get_stats().await
    }
}

fn media_engine_with_codecs(
    video_encoder_preferences: &Vec<VideoEncoderOptions>,
    audio_encoder_preferences: &[AudioEncoderOptions],
) -> webrtc::error::Result<MediaEngine> {
    let mut media_engine = MediaEngine::default();

    // Opus is the only supported codec. The only negotiable option in AudioEncoderOptions is FEC.
    // Since FEC is the only variant, we can just check the first option’s FEC value
    // and register Opus with/without FEC accordingly, in the preferred order.
    // Channels field is the same for all encoder preferences.
    if let Some(AudioEncoderOptions::Opus(opts)) = audio_encoder_preferences.first() {
        let channels = match opts.channels {
            AudioChannels::Mono => 1,
            AudioChannels::Stereo => 2,
        };
        register_opus_for_both_fec(
            &mut media_engine,
            opts.sample_rate,
            channels,
            opts.forward_error_correction,
        )?;
    } else {
        // default Opus register to make Twitch work
        register_opus_for_both_fec(&mut media_engine, 48000, 2, true)?;
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

    for encoder_options in video_encoder_preferences {
        match encoder_options {
            VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_) => {
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
            VideoEncoderOptions::FfmpegVp8(_) => {
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
            VideoEncoderOptions::FfmpegVp9(_) => {
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

fn register_opus_for_both_fec(
    media_engine: &mut MediaEngine,
    sample_rate: u32,
    channels: u16,
    fec_first: bool,
) -> webrtc::error::Result<()> {
    media_engine.register_codec(
        create_opus_codec_params(sample_rate, channels, fec_first),
        RTPCodecType::Audio,
    )?;

    media_engine.register_codec(
        create_opus_codec_params(sample_rate, channels, !fec_first),
        RTPCodecType::Audio,
    )?;

    Ok(())
}

fn create_opus_codec_params(
    sample_rate: u32,
    channels: u16,
    fec_enabled: bool,
) -> RTCRtpCodecParameters {
    let (payload_type, fec) = if fec_enabled { (111, 1) } else { (110, 0) };

    RTCRtpCodecParameters {
        capability: RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: sample_rate,
            channels,
            sdp_fmtp_line: format!("minptime=10;useinbandfec={fec}"),
            rtcp_feedback: vec![],
        },
        payload_type,
        ..Default::default()
    }
}
