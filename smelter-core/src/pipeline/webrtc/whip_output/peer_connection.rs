use tracing::debug;
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
    },
    ice_transport::{
        ice_connection_state::RTCIceConnectionState, ice_gatherer::OnLocalCandidateHdlrFn,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::{
        RTCRtpTransceiverInit,
        rtp_codec::{RTCRtpCodecParameters, RTPCodecType},
        rtp_sender::RTCRtpSender,
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
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
        video_codecs: &[RTCRtpCodecParameters],
        audio_codecs: &[RTCRtpCodecParameters],
    ) -> Result<Self, WebrtcClientError> {
        let mut media_engine = media_engine_with_codecs(video_codecs, audio_codecs)?;
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

    pub async fn new_video_track(&self) -> Result<Arc<RTCRtpSender>, WebrtcClientError> {
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
            .map_err(WebrtcClientError::PeerConnectionInitError)?;
        let sender = transceiver.sender().await;
        let rtc_sender_params = sender.get_parameters().await;
        debug!("RTCRtpSender video params: {:#?}", rtc_sender_params);
        Ok(sender)
    }

    pub async fn new_audio_track(&self) -> Result<Arc<RTCRtpSender>, WebrtcClientError> {
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
            .map_err(WebrtcClientError::PeerConnectionInitError)?;
        let sender = transceiver.sender().await;
        let rtc_sender_params = sender.get_parameters().await;
        debug!("RTCRtpSender audio params: {:#?}", rtc_sender_params);
        Ok(sender)
    }

    pub async fn set_remote_description(
        &self,
        answer: RTCSessionDescription,
    ) -> Result<(), WebrtcClientError> {
        self.pc
            .set_remote_description(answer)
            .await
            .map_err(WebrtcClientError::RemoteDescriptionError)
    }

    pub async fn set_local_description(
        &self,
        offer: RTCSessionDescription,
    ) -> Result<(), WebrtcClientError> {
        self.pc
            .set_local_description(offer)
            .await
            .map_err(WebrtcClientError::LocalDescriptionError)
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription, WebrtcClientError> {
        self.pc
            .create_offer(None)
            .await
            .map_err(WebrtcClientError::OfferCreationError)
    }

    pub fn on_ice_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.pc.on_ice_candidate(f);
    }

    pub async fn get_stats(&self) -> StatsReport {
        self.pc.get_stats().await
    }
}

fn media_engine_with_codecs(
    video_codecs: &[RTCRtpCodecParameters],
    audio_codecs: &[RTCRtpCodecParameters],
) -> webrtc::error::Result<MediaEngine> {
    let mut media_engine = MediaEngine::default();

    for audio_codec in audio_codecs {
        media_engine.register_codec(audio_codec.clone(), RTPCodecType::Audio)?;
    }

    for video_codec in video_codecs {
        media_engine.register_codec(video_codec.clone(), RTPCodecType::Video)?;
    }

    Ok(media_engine)
}
