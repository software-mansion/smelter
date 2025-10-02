use std::sync::Arc;

use tracing::{debug, warn};
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors, media_engine::MediaEngine, APIBuilder,
    },
    ice_transport::{
        ice_connection_state::RTCIceConnectionState, ice_gatherer::OnLocalCandidateHdlrFn,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription,
        OnTrackHdlrFn, RTCPeerConnection,
    },
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecParameters, RTPCodecType},
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
        RTCRtpTransceiver, RTCRtpTransceiverInit,
    },
};

use crate::{
    pipeline::{webrtc::supported_video_codec_parameters::get_audio_opus_codec, PipelineCtx},
    prelude::WebrtcClientError,
};

#[derive(Debug, Clone)]
pub(crate) struct PeerConnection {
    pc: Arc<RTCPeerConnection>,
}

impl PeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        video_codecs: &Vec<RTCRtpCodecParameters>,
    ) -> Result<Self, WebrtcClientError> {
        let mut media_engine = media_engine_with_codecs(video_codecs)?;
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

    pub async fn new_video_track(
        &self,
        video_codecs: &[RTCRtpCodecParameters],
    ) -> Result<Arc<RTCRtpTransceiver>, WebrtcClientError> {
        let transceiver = self
            .pc
            .add_transceiver_from_kind(
                RTPCodecType::Video,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    send_encodings: vec![],
                }),
            )
            .await?;

        if let Err(err) = transceiver
            .set_codec_preferences(video_codecs.to_vec())
            .await
        {
            warn!("Cannot set codec preferences for sdp answer: {err:?}");
        }
        Ok(transceiver)
    }

    pub async fn new_audio_track(&self) -> Result<Arc<RTCRtpTransceiver>, WebrtcClientError> {
        let transceiver = self
            .pc
            .add_transceiver_from_kind(
                RTPCodecType::Audio,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    send_encodings: vec![],
                }),
            )
            .await?;
        Ok(transceiver)
    }

    pub async fn set_remote_description(
        &self,
        answer: RTCSessionDescription,
    ) -> Result<(), WebrtcClientError> {
        Ok(self.pc.set_remote_description(answer).await?)
    }

    pub async fn set_local_description(
        &self,
        offer: RTCSessionDescription,
    ) -> Result<(), WebrtcClientError> {
        Ok(self.pc.set_local_description(offer).await?)
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription, WebrtcClientError> {
        Ok(self.pc.create_offer(None).await?)
    }

    pub fn on_ice_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.pc.on_ice_candidate(f);
    }

    pub fn on_track(&self, f: OnTrackHdlrFn) {
        self.pc.on_track(f);
    }
}

fn media_engine_with_codecs(
    video_codecs: &Vec<RTCRtpCodecParameters>,
) -> webrtc::error::Result<MediaEngine> {
    let mut media_engine = MediaEngine::default();

    for audio_codec in get_audio_opus_codec() {
        media_engine.register_codec(audio_codec.clone(), RTPCodecType::Audio)?;
    }

    for video_codec in video_codecs {
        media_engine.register_codec(video_codec.clone(), RTPCodecType::Video)?;
    }

    Ok(media_engine)
}
