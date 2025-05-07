use std::sync::Arc;

use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_OPUS},
        APIBuilder,
    },
    ice_transport::ice_server::RTCIceServer,
    interceptor::registry::Registry,
    peer_connection::{configuration::RTCConfiguration, RTCPeerConnection},
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType},
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
        RTCRtpTransceiver, RTCRtpTransceiverInit,
    },
};

use crate::pipeline::VideoDecoder;

use super::{
    error::WhipServerError,
    supported_video_codec_parameters::{
        get_video_h264_codecs, get_video_vp8_codecs, get_video_vp9_codecs,
    },
};

pub async fn init_peer_connection(
    stun_servers: Vec<String>,
    video_decoder_preferences: Vec<VideoDecoder>,
) -> Result<
    (
        Arc<RTCPeerConnection>,
        Arc<RTCRtpTransceiver>,
        Arc<RTCRtpTransceiver>,
    ),
    WhipServerError,
> {
    let mut media_engine = MediaEngine::default();

    register_codecs(&mut media_engine, video_decoder_preferences)?;

    let mut registry = Registry::new();

    registry = register_default_interceptors(registry, &mut media_engine)?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: stun_servers,
            ..Default::default()
        }],
        ..Default::default()
    };

    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    let video_transciver = peer_connection
        .add_transceiver_from_kind(
            RTPCodecType::Video,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                send_encodings: vec![],
            }),
        )
        .await?;

    let audio_transciver = peer_connection
        .add_transceiver_from_kind(
            RTPCodecType::Audio,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                send_encodings: vec![],
            }),
        )
        .await?;

    Ok((peer_connection, video_transciver, audio_transciver))
}

fn register_codecs(
    media_engine: &mut MediaEngine,
    video_decoder_preferences: Vec<VideoDecoder>,
) -> webrtc::error::Result<()> {
    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RTPCodecType::Audio,
    )?;

    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 1,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                rtcp_feedback: vec![],
            },
            payload_type: 112,
            ..Default::default()
        },
        RTPCodecType::Audio,
    )?;

    for video_decoder in video_decoder_preferences {
        match video_decoder {
            VideoDecoder::FFmpegH264 => {
                for codec in get_video_h264_codecs() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            #[cfg(feature = "vk-video")]
            VideoDecoder::VulkanVideoH264 => {
                for codec in get_video_h264_codecs() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoDecoder::FFmpegVp8 => {
                for codec in get_video_vp8_codecs() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoDecoder::FFmpegVp9 => {
                for codec in get_video_vp9_codecs() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
        }
    }

    Ok(())
}
