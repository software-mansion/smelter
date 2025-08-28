use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rand::Rng;
use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{sleep, timeout},
};
use tracing::debug;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
        APIBuilder,
    },
    ice_transport::{
        ice_candidate::RTCIceCandidateInit, ice_gatherer_state::RTCIceGathererState,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription, RTCPeerConnection,
    },
    rtp_transceiver::{
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType},
        rtp_sender::RTCRtpSender,
    },
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::pipeline::webrtc::{
    error::WhipWhepServerError,
    supported_video_codec_parameters::{
        get_video_h264_codecs_for_media_engine, get_video_vp8_codecs, get_video_vp9_codecs,
    },
};
use crate::prelude::*;

#[derive(Debug, Clone)]
pub(crate) struct PeerConnection {
    pc: Arc<RTCPeerConnection>,
    cleanup_task_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl PeerConnection {
    pub async fn new(
        ctx: &Arc<PipelineCtx>,
        video_encoder: &Option<VideoEncoderOptions>,
        audio_encoder: &Option<AudioEncoderOptions>,
    ) -> Result<Self, WhipWhepServerError> {
        let mut media_engine = MediaEngine::default();

        register_codecs(
            &mut media_engine,
            video_encoder.clone(),
            audio_encoder.clone(),
        )?;

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

        Ok(Self {
            pc: peer_connection,
            cleanup_task_handle: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn new_video_track(
        &self,
        encoder: &VideoEncoderOptions,
    ) -> Result<(Arc<TrackLocalStaticRTP>, Arc<RTCRtpSender>, u32), WhipWhepServerError> {
        let mime_type = match encoder {
            VideoEncoderOptions::FfmpegH264(_) => MIME_TYPE_H264,
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
            None => rand::thread_rng().gen::<u32>(),
        };

        Ok((track, sender, ssrc))
    }

    pub async fn new_audio_track(
        &self,
        encoder: &AudioEncoderOptions,
    ) -> Result<(Arc<TrackLocalStaticRTP>, u32), WhipWhepServerError> {
        let track = match encoder {
            AudioEncoderOptions::Opus(opts) => {
                let channels = match opts.channels {
                    AudioChannels::Mono => 1,
                    AudioChannels::Stereo => 2,
                };
                Arc::new(TrackLocalStaticRTP::new(
                    RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_OPUS.to_owned(),
                        clock_rate: 48000,
                        channels,
                        sdp_fmtp_line: "".to_owned(),
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
            None => rand::thread_rng().gen::<u32>(),
        };

        Ok((track, ssrc))
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
        offer: String,
    ) -> Result<RTCSessionDescription, WhipWhepServerError> {
        let offer = RTCSessionDescription::offer(offer)?;
        self.set_remote_description(offer).await?;

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

    pub async fn close(&self) -> Result<(), WhipWhepServerError> {
        Ok(self.pc.close().await?)
    }

    pub fn on_peer_connection_cleanup<F>(&self, cleanup_session_handler: F)
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + Clone
            + 'static,
    {
        let pc = self.pc.clone();
        let cleanup_task_handle = self.cleanup_task_handle.clone();

        self.pc.on_peer_connection_state_change(Box::new({
            move |state: RTCPeerConnectionState| {
                handle_cleanup_on_disconnect(
                    state,
                    pc.clone(),
                    cleanup_task_handle.clone(),
                    cleanup_session_handler.clone(),
                );
                Box::pin(async {})
            }
        }));
    }
}

fn register_codecs(
    media_engine: &mut MediaEngine,
    video_encoder: Option<VideoEncoderOptions>,
    audio_encoder: Option<AudioEncoderOptions>,
) -> Result<(), WhipWhepServerError> {
    if let Some(encoder) = video_encoder {
        match encoder {
            VideoEncoderOptions::FfmpegH264(_) => {
                for codec in get_video_h264_codecs_for_media_engine() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoEncoderOptions::FfmpegVp8(_) => {
                for codec in get_video_vp8_codecs() {
                    media_engine.register_codec(codec, RTPCodecType::Video)?;
                }
            }
            VideoEncoderOptions::FfmpegVp9(_) => {
                for codec in get_video_vp9_codecs() {
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
                media_engine.register_codec(
                    RTCRtpCodecParameters {
                        capability: RTCRtpCodecCapability {
                            mime_type: MIME_TYPE_OPUS.to_owned(),
                            clock_rate: 48000,
                            channels,
                            sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                            rtcp_feedback: vec![],
                        },
                        payload_type: 111,
                        ..Default::default()
                    },
                    RTPCodecType::Audio,
                )?;
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

fn handle_cleanup_on_disconnect<F>(
    state: RTCPeerConnectionState,
    pc: Arc<RTCPeerConnection>,
    cleanup_task_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    remove_session_handler: F,
) where
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
{
    match state {
        RTCPeerConnectionState::Connected => {
            if let Ok(mut handle) = cleanup_task_handle.lock() {
                if let Some(task) = handle.take() {
                    task.abort();
                }
            }
        }
        RTCPeerConnectionState::Failed | RTCPeerConnectionState::Disconnected => {
            if let Ok(handle @ None) = cleanup_task_handle.lock().as_deref_mut() {
                // schedule task only if none is pending, crucial in transitions failed <-> disconnected
                let task = tokio::spawn(maybe_cleanup_peer_connection(pc, remove_session_handler));
                *handle = Some(task);
            }
        }
        _ => {
            // Other states aren't crucial for cleanup
        }
    }
}

async fn maybe_cleanup_peer_connection<F>(pc: Arc<RTCPeerConnection>, remove_session_handler: F)
where
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync
        + 'static,
{
    let cleanup_delay = Duration::from_secs(150); // 2 min 30 s
    sleep(cleanup_delay).await;

    let current_state = pc.connection_state();
    if current_state != RTCPeerConnectionState::Connected
        && current_state != RTCPeerConnectionState::Connecting
    {
        remove_session_handler().await;
    }
}
