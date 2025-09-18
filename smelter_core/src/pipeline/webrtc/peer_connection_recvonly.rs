use std::{sync::Arc, time::Duration};

use tokio::{sync::watch, time::timeout};
use tracing::{debug, warn};
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_OPUS},
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
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType},
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
        RTCRtpTransceiver, RTCRtpTransceiverInit,
    },
};

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::{
        webrtc::{
            error::WhipWhepServerError,
            supported_video_codec_parameters::{
                get_video_h264_codecs_for_codec_preferences,
                get_video_h264_codecs_for_media_engine, get_video_vp8_codecs, get_video_vp9_codecs,
            },
        },
        PipelineCtx,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct RecvonlyPeerConnection {
    pc: Arc<RTCPeerConnection>,
}

impl RecvonlyPeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        video_preferences: &Vec<VideoDecoderOptions>,
    ) -> Result<Self, WhipWhepServerError> {
        let mut media_engine = media_engine_with_codecs(video_preferences)?;
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
        video_preferences: &Vec<VideoDecoderOptions>,
    ) -> Result<Arc<RTCRtpTransceiver>, WhipWhepServerError> {
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
            .set_codec_preferences(map_video_decoder_to_rtp_codec_parameters(video_preferences))
            .await
        {
            warn!("Cannot set codec preferences for sdp answer: {err:?}");
        }
        Ok(transceiver)
    }

    pub async fn new_audio_track(&self) -> Result<Arc<RTCRtpTransceiver>, WhipWhepServerError> {
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

fn media_engine_with_codecs(
    video_preferences: &Vec<VideoDecoderOptions>,
) -> webrtc::error::Result<MediaEngine> {
    let mut media_engine = MediaEngine::default();
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

    for video_decoder in video_preferences {
        match video_decoder {
            VideoDecoderOptions::FfmpegH264 => {
                for codec in get_video_h264_codecs_for_media_engine() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoDecoderOptions::VulkanH264 => {
                for codec in get_video_h264_codecs_for_media_engine() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoDecoderOptions::FfmpegVp8 => {
                for codec in get_video_vp8_codecs() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoDecoderOptions::FfmpegVp9 => {
                for codec in get_video_vp9_codecs() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
        }
    }

    Ok(media_engine)
}

fn map_video_decoder_to_rtp_codec_parameters(
    video_preferences: &Vec<VideoDecoderOptions>,
) -> Vec<RTCRtpCodecParameters> {
    let video_vp8_codec = get_video_vp8_codecs();
    let video_vp9_codec = get_video_vp9_codecs();
    let video_h264_codecs = get_video_h264_codecs_for_codec_preferences();

    let mut codec_list = Vec::new();

    for decoder in video_preferences {
        match decoder {
            VideoDecoderOptions::FfmpegH264 => {
                codec_list.extend(video_h264_codecs.clone());
            }
            VideoDecoderOptions::VulkanH264 => {
                codec_list.extend(video_h264_codecs.clone());
            }
            VideoDecoderOptions::FfmpegVp8 => {
                codec_list.extend(video_vp8_codec.clone());
            }
            VideoDecoderOptions::FfmpegVp9 => {
                codec_list.extend(video_vp9_codec.clone());
            }
        }
    }

    codec_list
}
