use std::{
    ops::Deref,
    sync::{Arc, Weak},
    time::Duration,
};

use tokio::{runtime::Handle, sync::watch, time::timeout};
use tracing::{debug, warn};
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
    },
    ice_transport::{
        ice_connection_state::RTCIceConnectionState, ice_gatherer_state::RTCIceGathererState,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{RTCPeerConnection, configuration::RTCConfiguration},
    rtp_transceiver::{
        RTCRtpTransceiver, RTCRtpTransceiverInit,
        rtp_codec::{RTCRtpCodecParameters, RTPCodecType},
        rtp_receiver::RTCRtpReceiver,
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    track::track_remote::TrackRemote,
};

use crate::pipeline::PipelineCtx;

#[derive(Debug, Clone)]
pub(crate) struct OnTrackHdlrContext {
    pub track: Arc<TrackRemote>,
    pub rtc_receiver: Arc<RTCRtpReceiver>,
}

#[derive(Debug)]
pub(crate) struct RecvonlyPeerConnection(Arc<Inner>);

/// Closes the peer connection when the last `RecvonlyPeerConnection` is dropped.
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

impl Deref for RecvonlyPeerConnection {
    type Target = RTCPeerConnection;

    fn deref(&self) -> &Self::Target {
        &self.0.pc
    }
}

impl RecvonlyPeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        video_codecs: &[RTCRtpCodecParameters],
        audio_codecs: &[RTCRtpCodecParameters],
    ) -> Result<Self, webrtc::Error> {
        let mut media_engine = media_engine_with_codecs(video_codecs, audio_codecs)?;
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

    pub async fn new_video_track(
        &self,
        video_codecs: &[RTCRtpCodecParameters],
    ) -> Result<Arc<RTCRtpTransceiver>, webrtc::Error> {
        let transceiver = self
            .0
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
            .0
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

    pub async fn wait_for_ice_candidates(
        &self,
        wait_timeout: Duration,
    ) -> Result<(), webrtc::Error> {
        let (sender, mut receiver) = watch::channel(RTCIceGathererState::Unspecified);

        self.0
            .pc
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

    pub fn on_track<F: FnMut(OnTrackHdlrContext) + Send + Sync + 'static>(&self, mut f: F) {
        self.0.pc.on_track(Box::new(move |track, rtc_receiver, _| {
            let ctx = OnTrackHdlrContext {
                track,
                rtc_receiver,
            };
            f(ctx);
            Box::pin(async {})
        }));
    }

    pub fn downgrade(&self) -> WeakRecvonlyPeerConnection {
        WeakRecvonlyPeerConnection(Arc::downgrade(&self.0))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WeakRecvonlyPeerConnection(Weak<Inner>);

impl WeakRecvonlyPeerConnection {
    pub fn upgrade(&self) -> Option<RecvonlyPeerConnection> {
        self.0.upgrade().map(RecvonlyPeerConnection)
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
