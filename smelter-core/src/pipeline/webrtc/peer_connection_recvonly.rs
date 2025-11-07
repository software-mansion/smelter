use std::{sync::Arc, time::Duration};

use tokio::{sync::watch, time::timeout};
use tracing::{debug, warn};
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
    },
    ice_transport::{
        ice_candidate::RTCIceCandidateInit, ice_connection_state::RTCIceConnectionState,
        ice_gatherer::OnLocalCandidateHdlrFn, ice_gatherer_state::RTCIceGathererState,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::{
        RTCRtpTransceiver, RTCRtpTransceiverInit,
        rtp_codec::{RTCRtpCodecParameters, RTPCodecType},
        rtp_receiver::RTCRtpReceiver,
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    track::track_remote::TrackRemote,
};

use crate::{
    AudioChannels,
    pipeline::{PipelineCtx, webrtc::supported_codec_parameters::opus_codec_params},
};

#[derive(Debug, Clone)]
pub(crate) struct OnTrackHdlrContext {
    pub track: Arc<TrackRemote>,
    pub rtc_receiver: Arc<RTCRtpReceiver>,
}

#[derive(Debug, Clone)]
pub(crate) struct RecvonlyPeerConnection {
    pc: Arc<RTCPeerConnection>,
}

impl RecvonlyPeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        video_codecs: &[RTCRtpCodecParameters],
    ) -> Result<Self, webrtc::Error> {
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

    pub fn connection_state(&self) -> RTCPeerConnectionState {
        self.pc.connection_state()
    }

    pub async fn close(&self) -> Result<(), webrtc::Error> {
        self.pc.close().await
    }

    pub async fn new_video_track(
        &self,
        video_codecs: &[RTCRtpCodecParameters],
    ) -> Result<Arc<RTCRtpTransceiver>, webrtc::Error> {
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

        // When setting codec preferences, payload types should be compatible with those in the offer. Simplest way to achieve that is by setting defaults
        let codec_preferences = video_codecs
            .iter()
            .map(|codec| RTCRtpCodecParameters {
                capability: codec.capability.clone(),
                ..Default::default()
            })
            .collect();
        if let Err(err) = transceiver.set_codec_preferences(codec_preferences).await {
            warn!("Cannot set codec preferences for sdp answer: {err:?}");
        }
        Ok(transceiver)
    }

    pub async fn new_audio_track(&self) -> Result<Arc<RTCRtpTransceiver>, webrtc::Error> {
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
    ) -> Result<(), webrtc::Error> {
        self.pc.set_remote_description(answer).await
    }

    pub async fn set_local_description(
        &self,
        offer: RTCSessionDescription,
    ) -> Result<(), webrtc::Error> {
        self.pc.set_local_description(offer).await
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription, webrtc::Error> {
        self.pc.create_offer(None).await
    }

    pub async fn create_answer(&self) -> Result<RTCSessionDescription, webrtc::Error> {
        self.pc.create_answer(None).await
    }

    pub async fn local_description(&self) -> Option<RTCSessionDescription> {
        self.pc.local_description().await
    }

    pub async fn wait_for_ice_candidates(
        &self,
        wait_timeout: Duration,
    ) -> Result<(), webrtc::Error> {
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

    pub fn on_ice_candidate(&self, f: OnLocalCandidateHdlrFn) {
        self.pc.on_ice_candidate(f);
    }

    pub fn on_track<F: FnMut(OnTrackHdlrContext) + Send + Sync + 'static>(&self, mut f: F) {
        self.pc.on_track(Box::new(move |track, rtc_receiver, _| {
            let ctx = OnTrackHdlrContext {
                track,
                rtc_receiver,
            };
            f(ctx);
            Box::pin(async {})
        }));
    }

    pub async fn add_ice_candidate(
        &self,
        candidate: RTCIceCandidateInit,
    ) -> Result<(), webrtc::Error> {
        self.pc.add_ice_candidate(candidate).await
    }
}

fn media_engine_with_codecs(
    video_codecs: &[RTCRtpCodecParameters],
) -> webrtc::error::Result<MediaEngine> {
    let mut media_engine = MediaEngine::default();

    // our decoder supports only stereo
    for audio_codec in opus_codec_params(true /* fec_first */, AudioChannels::Stereo) {
        media_engine.register_codec(audio_codec.clone(), RTPCodecType::Audio)?;
    }

    for video_codec in video_codecs {
        media_engine.register_codec(video_codec.clone(), RTPCodecType::Video)?;
    }

    Ok(media_engine)
}
