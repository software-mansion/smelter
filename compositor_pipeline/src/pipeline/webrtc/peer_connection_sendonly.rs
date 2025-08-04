use std::{sync::Arc, time::Duration};

use tokio::{sync::watch, time::timeout};
use tracing::debug;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS},
        APIBuilder,
    },
    ice_transport::{
        ice_candidate::RTCIceCandidateInit, ice_connection_state::RTCIceConnectionState,
        ice_gatherer_state::RTCIceGathererState, ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription, OnTrackHdlrFn, RTCPeerConnection,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::{
    codecs::{AudioEncoderOptions, VideoEncoderOptions},
    pipeline::{webrtc::error::WhipWhepServerError, PipelineCtx},
};

#[derive(Debug, Clone)]
pub(crate) struct SendonlyPeerConnection {
    pc: Arc<RTCPeerConnection>,
}

impl SendonlyPeerConnection {
    pub async fn new(ctx: &Arc<PipelineCtx>) -> Result<Self, WhipWhepServerError> {
        // let mut media_engine = media_engine_with_codecs(video_preferences)?;
        let mut media_engine = MediaEngine::default();
        let _ = media_engine.register_default_codecs();
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

    pub fn connection_state(&self) -> RTCPeerConnectionState {
        self.pc.connection_state()
    }

    pub async fn close(&self) -> Result<(), WhipWhepServerError> {
        Ok(self.pc.close().await?)
    }

    pub async fn new_video_track(
        &self,
        encoder: VideoEncoderOptions,
    ) -> Result<Arc<TrackLocalStaticRTP>, WhipWhepServerError> {
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            "video".to_string(),
            "webrtc".to_string(),
        ));
        self.pc.add_track(track.clone()).await?;

        Ok(track)
    }

    pub async fn new_audio_track(
        &self,
        encoder: AudioEncoderOptions,
    ) -> Result<Arc<TrackLocalStaticRTP>, WhipWhepServerError> {
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            "audio".to_string(),
            "webrtc".to_string(),
        ));
        self.pc.add_track(track.clone()).await?;

        Ok(track)
    }

    pub async fn set_remote_description(
        &self,
        answer: RTCSessionDescription,
    ) -> Result<(), WhipWhepServerError> {
        Ok(self.pc.set_remote_description(answer).await?)
    }

    pub async fn set_local_description(
        &self,
        offer: RTCSessionDescription,
    ) -> Result<(), WhipWhepServerError> {
        Ok(self.pc.set_local_description(offer).await?)
    }

    pub async fn create_answer(&self) -> Result<RTCSessionDescription, WhipWhepServerError> {
        Ok(self.pc.create_answer(None).await?)
    }

    pub async fn local_description(&self) -> Result<RTCSessionDescription, WhipWhepServerError> {
        match self.pc.local_description().await {
            Some(dsc) => Ok(dsc),
            None => Err(WhipWhepServerError::InternalError(
                "Local description is not set, cannot read it".to_string(),
            )),
        }
    }

    pub async fn wait_for_ice_candidates(
        &self,
        wait_timeout: Duration,
    ) -> Result<(), WhipWhepServerError> {
        let (sender, mut receiver) = watch::channel(RTCIceGathererState::Unspecified);

        self.pc
            .on_ice_gathering_state_change(Box::new(move |gatherer_state| {
                if let Err(err) = sender.send(gatherer_state) {
                    debug!("Cannot send gathering state: {err:?}");
                };
                Box::pin(async {})
            }));

        let gather_candidates = async {
            while receiver.changed().await.is_ok() {
                if *receiver.borrow() == RTCIceGathererState::Complete {
                    break;
                }
            }
        };

        if timeout(wait_timeout, gather_candidates).await.is_err() {
            debug!("Maximum time for gathering candidate has elapsed.");
        }
        Ok(())
    }

    pub fn on_track(&self, f: OnTrackHdlrFn) {
        self.pc.on_track(f);
    }

    pub async fn add_ice_candidate(
        &self,
        candidate: RTCIceCandidateInit,
    ) -> Result<(), WhipWhepServerError> {
        Ok(self.pc.add_ice_candidate(candidate).await?)
    }
}
