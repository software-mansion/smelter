use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use rand::Rng;
use tokio::{sync::watch, time::timeout};
use tracing::debug;
use webrtc::{
    api::{
        APIBuilder,
        interceptor_registry::register_default_interceptors,
        media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9, MediaEngine},
    },
    ice_transport::{
        ice_candidate::RTCIceCandidateInit, ice_gatherer_state::RTCIceGathererState,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecCapability, RTPCodecType},
        rtp_sender::RTCRtpSender,
    },
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::pipeline::webrtc::{error::WhipWhepServerError, offer_codec_filter::codecs_from_offer};

use crate::prelude::*;

use super::pc_state_change::ConnectionStateChangeHdlr;

#[derive(Debug)]
pub(crate) struct PeerConnection {
    pc: Arc<RTCPeerConnection>,
}

impl PeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        video_encoder: &Option<VideoEncoderOptions>,
        audio_encoder: &Option<AudioEncoderOptions>,
        offer: &RTCSessionDescription,
    ) -> Result<Self, WhipWhepServerError> {
        let mut media_engine = MediaEngine::default();

        register_codecs(
            &mut media_engine,
            video_encoder.clone(),
            audio_encoder.clone(),
            offer,
        )?;

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

        Ok(Self {
            pc: peer_connection,
        })
    }

    pub async fn new_video_track(
        &self,
        encoder: &VideoEncoderOptions,
    ) -> Result<(Arc<TrackLocalStaticRTP>, Arc<RTCRtpSender>, u32), WhipWhepServerError> {
        let mime_type = match encoder {
            VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_) => {
                MIME_TYPE_H264
            }
            VideoEncoderOptions::FfmpegVp8(_) => MIME_TYPE_VP8,
            VideoEncoderOptions::FfmpegVp9(_) => MIME_TYPE_VP9,
        };
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: mime_type.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            "video".to_string(),
            "webrtc".to_string(),
        ));
        let sender = self.pc.add_track(track.clone()).await?;

        let rtc_sender_params = sender.get_parameters().await;
        let ssrc = match rtc_sender_params.encodings.first() {
            Some(e) => e.ssrc,
            None => rand::rng().random::<u32>(),
        };

        Ok((track, sender, ssrc))
    }

    pub async fn new_audio_track(
        &self,
        encoder: &AudioEncoderOptions,
    ) -> Result<(Arc<TrackLocalStaticRTP>, Arc<RTCRtpSender>, u32), WhipWhepServerError> {
        let track = match encoder {
            AudioEncoderOptions::Opus(opts) => {
                let channels = match opts.channels {
                    AudioChannels::Mono => 1,
                    AudioChannels::Stereo => 2,
                };
                let fec = opts.forward_error_correction;
                Arc::new(TrackLocalStaticRTP::new(
                    RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_OPUS.to_owned(),
                        clock_rate: opts.sample_rate,
                        channels,
                        sdp_fmtp_line: format!("minptime=10;useinbandfec={}", fec as u8).to_owned(),
                        rtcp_feedback: vec![],
                    },
                    "audio".to_string(),
                    "webrtc".to_string(),
                ))
            }
            AudioEncoderOptions::FdkAac(_) => {
                // this should never happen
                return Err(WhipWhepServerError::InternalError(
                    "AAC is not supported codec for WHEP output".to_owned(),
                ));
            }
        };

        let sender = self.pc.add_track(track.clone()).await?;

        let rtc_sender_params = sender.get_parameters().await;
        let ssrc = match rtc_sender_params.encodings.first() {
            Some(e) => e.ssrc,
            None => rand::rng().random::<u32>(),
        };

        Ok((track, sender, ssrc))
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

    pub async fn negotiate_connection(
        &self,
        offer: RTCSessionDescription,
        video_sender: Option<Arc<RTCRtpSender>>,
        audio_sender: Option<Arc<RTCRtpSender>>,
    ) -> Result<RTCSessionDescription, WhipWhepServerError> {
        self.set_remote_description(offer).await?;

        // allow audio/video only stream, when on second track codec wasn't succesfully negotiated
        cleanup_unnegotiated_tracks(video_sender, audio_sender).await?;

        let answer = self.create_answer().await?;
        self.set_local_description(answer).await?;

        self.wait_for_ice_candidates(Duration::from_secs(1)).await?;

        let sdp_answer = self.local_description().await?;

        Ok(sdp_answer)
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

    pub async fn add_ice_candidate(
        &self,
        candidate: RTCIceCandidateInit,
    ) -> Result<(), WhipWhepServerError> {
        Ok(self.pc.add_ice_candidate(candidate).await?)
    }

    pub fn on_connection_state_change(&self, handler: ConnectionStateChangeHdlr) {
        let pc = self.pc.clone();
        self.pc
            .on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
                handler.on_state_change(&pc, state);
                Box::pin(async {})
            }));
    }

    pub fn downgrade(&self) -> WeakPeerConnection {
        WeakPeerConnection {
            pc: Arc::downgrade(&self.pc),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WeakPeerConnection {
    pc: Weak<RTCPeerConnection>,
}

impl WeakPeerConnection {
    pub fn upgrade(&self) -> Option<PeerConnection> {
        self.pc.upgrade().map(|pc| PeerConnection { pc })
    }
}

impl Drop for PeerConnection {
    fn drop(&mut self) {
        if let Ok(handle) = tokio::runtime::Handle::try_current()
            && Arc::strong_count(&self.pc) == 1
        {
            let pc = self.pc.clone();
            handle.spawn(async move { pc.close().await });
        }
    }
}

fn register_codecs(
    media_engine: &mut MediaEngine,
    video_encoder: Option<VideoEncoderOptions>,
    audio_encoder: Option<AudioEncoderOptions>,
    offer: &RTCSessionDescription,
) -> Result<(), WhipWhepServerError> {
    let offer_codecs = codecs_from_offer(offer);
    if let Some(encoder) = video_encoder {
        match encoder {
            VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_) => {
                // We intentionally don't filter H264 codec subtypes from the offer.
                // We echo all offered H264 variants in the answer to maximize
                // negotiation success across clients. The encoder output is driven by
                // selected encoder implementation/options, not by negotiated H264
                // profile/level details from SDP.
                for codec in offer_codecs.h264 {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoEncoderOptions::FfmpegVp8(_) => {
                for codec in offer_codecs.vp8 {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoEncoderOptions::FfmpegVp9(_) => {
                for codec in offer_codecs.vp9 {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
        };
    };

    if let Some(encoder) = audio_encoder {
        match encoder {
            AudioEncoderOptions::Opus(opts) => {
                let channels = match opts.channels {
                    AudioChannels::Mono => 1,
                    AudioChannels::Stereo => 2,
                };
                for mut codec in offer_codecs.opus {
                    codec.capability.clock_rate = opts.sample_rate;
                    codec.capability.channels = channels;
                    media_engine.register_codec(codec, RTPCodecType::Audio)?;
                }
            }
            AudioEncoderOptions::FdkAac(_) => {
                return Err(WhipWhepServerError::InternalError(
                    "AAC is not supported codec for WHEP output".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

async fn cleanup_unnegotiated_tracks(
    video_sender: Option<Arc<RTCRtpSender>>,
    audio_sender: Option<Arc<RTCRtpSender>>,
) -> Result<(), WhipWhepServerError> {
    let mut any_codec_negotiated = false;
    match video_sender {
        Some(sender) if is_sender_codec_empty(&sender).await => sender.replace_track(None).await?,
        Some(_) => any_codec_negotiated = true,
        _ => {}
    }
    match audio_sender {
        Some(sender) if is_sender_codec_empty(&sender).await => sender.replace_track(None).await?,
        Some(_) => any_codec_negotiated = true,
        _ => {}
    }

    if !any_codec_negotiated {
        return Err(WhipWhepServerError::InternalError(
            "No codec was negotiated for either audio or video track".into(),
        ));
    }
    Ok(())
}

async fn is_sender_codec_empty(sender: &Arc<RTCRtpSender>) -> bool {
    sender
        .get_parameters()
        .await
        .rtp_parameters
        .codecs
        .is_empty()
}
