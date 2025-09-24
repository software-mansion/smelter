use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::pipeline::rtp::RtpNtpSyncPoint;
use crate::pipeline::webrtc::whep_input::peer_connection::PeerConnection;
use crate::pipeline::webrtc::whep_input::track_audio_thread::process_audio_track;
use crate::pipeline::webrtc::whep_input::track_video_thread::process_video_track;
use crate::pipeline::webrtc::whep_input::whep_http_client::{SdpAnswer, WhepHttpClient};
use crate::{pipeline::input::Input, queue::QueueDataReceiver};
use crossbeam_channel::{bounded, Sender};
use itertools::Itertools;
use smelter_render::error::ErrorStack;
use tokio::sync::oneshot;
use tracing::{debug, error, info, span, warn, Instrument, Level};
use url::Url;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecParameters, RTPCodecType};

use crate::pipeline::webrtc::supported_video_codec_parameters::{
    get_video_h264_codec, get_video_h264_codec_with_default_payload_type, get_video_vp8_codec,
    get_video_vp8_codec_with_default_payload_type, get_video_vp9_codec,
    get_video_vp9_codec_with_default_payload_type,
};
use crate::prelude::*;

mod peer_connection;
mod track_audio_thread;
mod track_video_thread;
mod whep_http_client;

const WHEP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct WhepInput;

impl WhepInput {
    pub(crate) fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        options: WhepInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let (init_confirmation_sender, init_confirmation_receiver) = oneshot::channel();

        let (frame_sender, frame_receiver) = bounded(5);
        let (input_samples_sender, input_samples_receiver) = bounded(5);

        let span = span!(
            Level::INFO,
            "WHEP client task",
            input_id = input_id.to_string()
        );
        let rt = ctx.tokio_rt.clone();
        rt.spawn(
            async {
                let result =
                    WhepClientTask::new(ctx, options, input_samples_sender, frame_sender).await;
                match result {
                    Ok(handle) => {
                        init_confirmation_sender.send(Ok(handle)).unwrap();
                    }
                    Err(err) => init_confirmation_sender.send(Err(err)).unwrap(),
                }
            }
            .instrument(span),
        );

        wait_with_deadline(init_confirmation_receiver, WHEP_INIT_TIMEOUT)?;
        Ok((
            Input::Whep(Self),
            InputInitInfo::Other,
            QueueDataReceiver {
                video: Some(frame_receiver),
                audio: Some(input_samples_receiver),
            },
        ))
    }
}

fn wait_with_deadline<T>(
    mut result_receiver: oneshot::Receiver<Result<T, WhepInputError>>,
    timeout: Duration,
) -> Result<T, InputInitError> {
    let start_time = Instant::now();
    while start_time.elapsed() < timeout {
        thread::sleep(Duration::from_millis(500));

        match result_receiver.try_recv() {
            Ok(result) => match result {
                Ok(handle) => return Ok(handle),
                Err(err) => return Err(InputInitError::WhepInitError(err.into())),
            },
            Err(err) => match err {
                oneshot::error::TryRecvError::Closed => {
                    return Err(InputInitError::UnknownWhepError)
                }
                oneshot::error::TryRecvError::Empty => {}
            },
        };
    }
    result_receiver.close();
    Err(InputInitError::WhepInitTimeout)
}

#[derive(Debug)]
struct WhepClientTask; //TODO refactor

impl WhepClientTask {
    async fn new(
        ctx: Arc<PipelineCtx>,
        options: WhepInputOptions,
        input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
        frame_sender: Sender<PipelineEvent<Frame>>,
    ) -> Result<Self, WhepInputError> {
        let client = WhepHttpClient::new(&options)?;
        let (video_preferences, video_codecs_to_register, video_pref) =
            resolve_video_preferences(&ctx, options.video_preferences)?;
        let pc = PeerConnection::new(&ctx, &video_codecs_to_register).await?;

        let _video_transceiver = pc.new_video_track(&video_pref).await?;
        let _audio_transceiver = pc.new_audio_track().await?;

        let (_session_url, answer) = exchange_sdp_offers(&pc, &client).await?;
        pc.set_remote_description(answer).await?;
        {
            let sync_point = RtpNtpSyncPoint::new(ctx.queue_sync_point);
            pc.on_track(Box::new(move |track, _, transceiver| {
                debug!(
                    kind=?track.kind(),
                    "on_track called"
                );

                let span = span!(Level::INFO, "WHEP input track", track_type=?track.kind());

                match track.kind() {
                    RTPCodecType::Audio => {
                        tokio::spawn(
                            process_audio_track(
                                ctx.clone(),
                                sync_point.clone(),
                                input_samples_sender.clone(),
                                track,
                                transceiver,
                            )
                            .instrument(span),
                        );
                    }
                    RTPCodecType::Video => {
                        tokio::spawn(
                            process_video_track(
                                ctx.clone(),
                                sync_point.clone(),
                                frame_sender.clone(),
                                track,
                                transceiver,
                                video_preferences.clone(),
                            )
                            .instrument(span),
                        );
                    }
                    RTPCodecType::Unspecified => {
                        warn!("Unknown track kind")
                    }
                }

                Box::pin(async {})
            }))
        };

        Ok(Self {})
    }
}

async fn exchange_sdp_offers(
    pc: &PeerConnection,
    client: &Arc<WhepHttpClient>,
) -> Result<(Url, RTCSessionDescription), WhepInputError> {
    let offer = pc.create_offer().await?;
    debug!("SDP offer: {}", offer.sdp);

    let SdpAnswer {
        session_url: location,
        answer,
    } = client.send_offer(&offer).await?;
    debug!("SDP answer: {}", answer.sdp);

    pc.set_local_description(offer).await?;

    listen_for_trickle_candidates(pc, client, location.clone());

    Ok((location, answer))
}

fn listen_for_trickle_candidates(pc: &PeerConnection, client: &Arc<WhepHttpClient>, location: Url) {
    let should_stop_trickle = Arc::new(AtomicBool::new(false));
    let location = location.clone();
    let client = client.clone();
    pc.on_ice_candidate(Box::new(move |candidate| {
        Box::pin(handle_trickle_candidate(
            client.clone(),
            candidate,
            location.clone(),
            should_stop_trickle.clone(),
        ))
    }));
}

async fn handle_trickle_candidate(
    client: Arc<WhepHttpClient>,
    candidate: Option<RTCIceCandidate>,
    location: Url,
    should_stop_trickle: Arc<AtomicBool>,
) {
    if should_stop_trickle.load(Ordering::Relaxed) {
        return;
    }
    let Some(candidate) = candidate else { return };
    let candidate = match candidate.to_json() {
        Ok(candidate) => candidate,
        Err(err) => {
            error!("Failed to process ICE candidate: {}", err);
            return;
        }
    };

    match client.send_trickle_ice(&location, candidate).await {
        Err(WhepInputError::TrickleIceNotSupported) => {
            info!("Trickle ICE is not supported by WHEP server");
            should_stop_trickle.store(true, Ordering::Relaxed);
        }
        Err(WhepInputError::EntityTagMissing) | Err(WhepInputError::EntityTagNonMatching) => {
            info!("Entity tags not supported by WHEP input");
            should_stop_trickle.store(true, Ordering::Relaxed);
        }
        Err(err) => warn!(
            "Trickle ICE request failed: {}",
            ErrorStack::new(&err).into_string()
        ),
        Ok(_) => (),
    };
}

#[allow(clippy::type_complexity)] //TODO
fn resolve_video_preferences(
    ctx: &Arc<PipelineCtx>,
    video_preferences: Vec<WebrtcVideoDecoderOptions>,
) -> Result<
    (
        Vec<VideoDecoderOptions>,
        Vec<RTCRtpCodecParameters>,
        Vec<RTCRtpCodecParameters>,
    ),
    WhepInputError,
> {
    let vulkan_supported = ctx.graphics_context.has_vulkan_support();
    let only_vulkan_in_preferences = video_preferences
        .iter()
        .all(|pref| matches!(pref, WebrtcVideoDecoderOptions::VulkanH264));
    if !vulkan_supported && only_vulkan_in_preferences {
        return Err(WhepInputError::DecoderInitError(
            DecoderInitError::VulkanContextRequiredForVulkanDecoder,
        ));
    };

    let video_preferences: Vec<VideoDecoderOptions> = video_preferences
        .into_iter()
        .flat_map(|preference| match preference {
            WebrtcVideoDecoderOptions::FfmpegH264 => vec![VideoDecoderOptions::FfmpegH264],
            WebrtcVideoDecoderOptions::VulkanH264 => {
                if vulkan_supported {
                    vec![VideoDecoderOptions::VulkanH264]
                } else {
                    warn!("Vulkan is not supported, skipping \"vulkan_h264\" preference");
                    vec![]
                }
            }
            WebrtcVideoDecoderOptions::FfmpegVp8 => vec![VideoDecoderOptions::FfmpegVp8],
            WebrtcVideoDecoderOptions::FfmpegVp9 => vec![VideoDecoderOptions::FfmpegVp9],
            WebrtcVideoDecoderOptions::Any => {
                vec![
                    VideoDecoderOptions::FfmpegVp9,
                    VideoDecoderOptions::FfmpegVp8,
                    if vulkan_supported {
                        VideoDecoderOptions::VulkanH264
                    } else {
                        VideoDecoderOptions::FfmpegH264
                    },
                ]
            }
        })
        .unique()
        .collect();

    // both necessary to work properly
    let mut video_codecs: Vec<RTCRtpCodecParameters> = Vec::new();
    let mut video_pref: Vec<RTCRtpCodecParameters> = Vec::new();
    for pref in &video_preferences {
        match pref {
            VideoDecoderOptions::FfmpegH264 | VideoDecoderOptions::VulkanH264 => {
                video_codecs.extend(get_video_h264_codec());
                video_pref.extend(get_video_h264_codec_with_default_payload_type());
            }
            VideoDecoderOptions::FfmpegVp8 => {
                video_codecs.extend(get_video_vp8_codec());
                video_pref.extend(get_video_vp8_codec_with_default_payload_type());
            }
            VideoDecoderOptions::FfmpegVp9 => {
                video_codecs.extend(get_video_vp9_codec());
                video_pref.extend(get_video_vp9_codec_with_default_payload_type());
            }
        }
    }

    let video_codecs = video_codecs
        .into_iter()
        .unique_by(|c| {
            (
                c.capability.mime_type.clone(),
                c.capability.sdp_fmtp_line.clone(),
            )
        })
        .collect();
    let video_pref = video_pref
        .into_iter()
        .unique_by(|c| {
            (
                c.capability.mime_type.clone(),
                c.capability.sdp_fmtp_line.clone(),
            )
        })
        .collect();
    Ok((video_preferences, video_codecs, video_pref))
}
