use tokio::runtime::Handle;
use tracing::{debug, warn};
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
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::{
        RTCRtpTransceiverInit, rtp_codec::RTPCodecType, rtp_sender::RTCRtpSender,
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    stats::StatsReport,
};

use std::sync::{Arc, Weak};

use crate::pipeline::webrtc::whip_output::codec_preferences::CodecParameters;

use crate::prelude::*;

#[derive(Debug)]
pub(super) struct PeerConnection(Arc<Inner>);

/// Closes the peer connection when the last `PeerConnection` is dropped.
#[derive(Debug)]
struct Inner {
    pc: Arc<RTCPeerConnection>,
    tokio_rt: Handle,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let pc = self.pc.clone();
        self.tokio_rt.spawn(async move {
            if let Err(err) = pc.close().await {
                warn!(%err, "Failed to close peer connection.");
            }
        });
    }
}

impl PeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        codec_params: CodecParameters,
    ) -> Result<Self, WebrtcClientError> {
        let mut media_engine = MediaEngine::default();
        for audio_codec in codec_params.audio_codecs {
            media_engine.register_codec(audio_codec.clone(), RTPCodecType::Audio)?;
        }
        for video_codec in codec_params.video_codecs {
            media_engine.register_codec(video_codec.clone(), RTPCodecType::Video)?;
        }

        let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_setting_engine(ctx.webrtc_setting_engine.create_setting_engine())
            .build();

        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: ctx.webrtc_stun_servers.to_vec(),
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

        Ok(Self(Arc::new(Inner {
            pc: peer_connection,
            tokio_rt: ctx.tokio_rt.clone(),
        })))
    }

    pub fn on_connection_state_change(
        &self,
        f: impl Fn(RTCPeerConnectionState) + Send + Sync + 'static,
    ) {
        self.0.pc.on_peer_connection_state_change(Box::new(
            move |state: RTCPeerConnectionState| {
                f(state);
                Box::pin(async {})
            },
        ));
    }

    pub async fn new_video_track(&self) -> Result<Arc<RTCRtpSender>, WebrtcClientError> {
        let transceiver = self
            .0
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
            .0
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
        self.0
            .pc
            .set_remote_description(answer)
            .await
            .map_err(WebrtcClientError::RemoteDescriptionError)
    }

    pub async fn set_local_description(
        &self,
        offer: RTCSessionDescription,
    ) -> Result<(), WebrtcClientError> {
        self.0
            .pc
            .set_local_description(offer)
            .await
            .map_err(WebrtcClientError::LocalDescriptionError)
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription, WebrtcClientError> {
        self.0
            .pc
            .create_offer(None)
            .await
            .map_err(WebrtcClientError::OfferCreationError)
    }

    pub fn on_ice_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.0.pc.on_ice_candidate(f);
    }

    pub async fn get_stats(&self) -> StatsReport {
        self.0.pc.get_stats().await
    }

    pub fn downgrade(&self) -> WeakPeerConnection {
        WeakPeerConnection(Arc::downgrade(&self.0))
    }
}

#[derive(Debug, Clone)]
pub(super) struct WeakPeerConnection(Weak<Inner>);

impl WeakPeerConnection {
    pub fn upgrade(&self) -> Option<PeerConnection> {
        self.0.upgrade().map(PeerConnection)
    }
}
