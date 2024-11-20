use super::WhipError;
use std::{
    env::{self, VarError},
    sync::Arc,
};
use tracing::{info, warn};
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS},
        APIBuilder,
    },
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    peer_connection::{configuration::RTCConfiguration, RTCPeerConnection},
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType},
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

const STUN_SERVER_ENV: &str = "LIVE_COMPOSITOR_STUN_SERVERS";

pub async fn init_peer_connection() -> Result<
    (
        Arc<RTCPeerConnection>,
        Arc<TrackLocalStaticRTP>,
        Arc<TrackLocalStaticRTP>,
    ),
    WhipError,
> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 96,
            ..Default::default()
        },
        RTPCodecType::Video,
    )?;
    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RTPCodecType::Audio,
    )?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let mut stun_servers_urls = vec!["stun:stun.l.google.com:19302".to_owned()];

    match env::var(STUN_SERVER_ENV) {
        Ok(var) => {
            if var.is_empty() {
                info!("Empty LIVE_COMPOSITOR_STUN_SERVERS environment variable, using default");
            } else {
                let env_url_list: Vec<String> = var.split(',').map(String::from).collect();
                stun_servers_urls.extend(env_url_list);
                info!("Using custom stun servers defined in LIVE_COMPOSITOR_STUN_SERVERS environment variable");
            }
        }
        Err(err) => match err {
            VarError::NotPresent => info!("No stun servers provided, using default"),
            VarError::NotUnicode(_) => warn!("Invalid LIVE_COMPOSITOR_STUN_SERVERS environment variable, it is not a valid Unicode, using default")
        },
    }

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: stun_servers_urls,
            ..Default::default()
        }],
        ..Default::default()
    };
    let peer_connection = Arc::new(api.new_peer_connection(config).await?);
    let video_track = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    let audio_track = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "audio".to_owned(),
        "webrtc-rs".to_owned(),
    ));
    peer_connection
        .add_track(video_track.clone())
        .await
        .map_err(WhipError::PeerConnectionInitError)?;
    peer_connection
        .add_track(audio_track.clone())
        .await
        .map_err(WhipError::PeerConnectionInitError)?;
    let transceivers = peer_connection.get_transceivers().await;
    for transceiver in transceivers {
        transceiver
            .set_direction(RTCRtpTransceiverDirection::Sendonly)
            .await;
    }
    Ok((peer_connection, video_track, audio_track))
}
