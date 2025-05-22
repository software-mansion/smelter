use compositor_render::OutputId;
use crossbeam_channel::Sender;
use establish_peer_connection::{connect, exchange_sdp_offers};

use init_encoder_after_negotiation::create_encoder_and_packet_stream;
use payloader::{Payload, PayloadingError};
use peer_connection::PeerConnection;
use reqwest::{Method, StatusCode};
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use setup_track::setup_track;
use std::{
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, span, Instrument, Level};
use track_task_audio::WhipAudioTrackThreadHandle;
use track_task_video::WhipVideoTrackThreadHandle;
use url::{ParseError, Url};
use webrtc::{
    rtp_transceiver::rtp_sender::RTCRtpSender,
    track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter},
};
use whip_http_client::WhipHttpClient;

use crate::{
    error::OutputInitError,
    event::Event,
    pipeline::{
        encoder::{AudioEncoderOptions, VideoEncoderOptions},
        PipelineCtx,
    },
    queue::PipelineEvent,
};

mod establish_peer_connection;
mod init_encoder_after_negotiation;
mod payloader;
mod setup_track;

mod peer_connection;
mod track_task_audio;
mod track_task_video;
mod whip_http_client;

pub struct WhipClientOutput {
    //   pub connection_options: WhipSenderOptions,
    should_close: Arc<AtomicBool>,
    pub video: Option<WhipVideoTrackThreadHandle>,
    pub audio: Option<WhipAudioTrackThreadHandle>,
}

#[derive(Debug, Clone)]
pub struct VideoWhipOptions {
    pub encoder_preferences: Vec<VideoEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct AudioWhipOptions {
    pub encoder_preferences: Vec<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct WhipSenderOptions {
    pub endpoint_url: Arc<str>,
    pub bearer_token: Option<Arc<str>>,
    pub video: Option<VideoWhipOptions>,
    pub audio: Option<AudioWhipOptions>,
}

#[derive(Debug, Clone)]
pub struct WhipCtx {
    output_id: OutputId,
    options: WhipSenderOptions,
    should_close: Arc<AtomicBool>,
    pipeline_ctx: Arc<PipelineCtx>,
    client: Arc<WhipHttpClient>,
}

const WHIP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

impl WhipClientOutput {
    pub fn new(
        pipeline_ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: WhipSenderOptions,
    ) -> Result<Self, OutputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));
        let whip_ctx = Arc::new(WhipCtx {
            output_id: output_id.clone(),
            should_close: should_close.clone(),
            pipeline_ctx: pipeline_ctx.clone(),
            client: WhipHttpClient::new(&options.endpoint_url, &options.bearer_token)
                .map_err(|err| OutputInitError::WhipInitError(err.into()))?
                .into(),
            options,
        });

        let handle = Self::spawn_whip_task(whip_ctx)?;

        Ok(Self {
            should_close,
            video,
            audio,
        })
    }

    fn spawn_whip_task(whip_ctx: Arc<WhipCtx>) -> Result<WhipSenderTaskHandle, OutputInitError> {
        let (init_confirmation_sender, init_confirmation_receiver) =
            oneshot::channel::<Result<WhipSenderTaskHandle, WhipError>>();

        let output_id = whip_ctx.output_id.clone();
        let rt = whip_ctx.pipeline_ctx.tokio_rt.clone();
        rt.spawn(
            async {
                let result = WhipSenderTask::new(whip_ctx).await;
                match result {
                    Ok(handle) => init_confirmation_sender.send(Ok(handle)),
                    Err(err) => init_confirmation_sender.send(Err(err)),
                }
            }
            .instrument(span!(
                Level::INFO,
                "WHIP sender task",
                output_id = output_id.to_string()
            )),
        );

        Self::wait_with_deadline(init_confirmation_receiver)
    }

    fn wait_with_deadline(
        mut result_receiver: oneshot::Receiver<Result<WhipSenderTaskHandle, WhipError>>,
    ) -> Result<WhipSenderTaskHandle, OutputInitError> {
        let start_time = Instant::now();
        while start_time.elapsed() < WHIP_INIT_TIMEOUT {
            thread::sleep(Duration::from_millis(500));

            match result_receiver.try_recv() {
                Ok(result) => match result {
                    Ok(handle) => return Ok(handle),
                    Err(err) => return Err(OutputInitError::WhipInitError(err.into())),
                },
                Err(err) => match err {
                    oneshot::error::TryRecvError::Closed => {
                        return Err(OutputInitError::UnknownWhipError)
                    }
                    oneshot::error::TryRecvError::Empty => {}
                },
            };
        }
        result_receiver.close();
        Err(OutputInitError::WhipInitTimeout)
    }
}

impl Drop for WhipClientOutput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

struct WhipSenderTask {
    session_url: Url,
    ctx: Arc<PipelineCtx>,
    client: Arc<WhipHttpClient>,
    output_id: OutputId,
    video: Option<(
        mpsc::Receiver<PipelineEvent<rtp::packet::Packet>>,
        Arc<TrackLocalStaticRTP>,
    )>,
    audio: Option<(
        mpsc::Receiver<PipelineEvent<rtp::packet::Packet>>,
        Arc<TrackLocalStaticRTP>,
    )>,
}

struct WhipSenderTaskHandle {
    video: Option<WhipVideoTrackThreadHandle>,
    audio: Option<WhipAudioTrackThreadHandle>,
}

impl WhipSenderTask {
    async fn new(whip_ctx: Arc<WhipCtx>) -> Result<(Self, WhipSenderTaskHandle), WhipError> {
        let peer_connection = PeerConnection::new(&whip_ctx).await?;

        let video = match whip_ctx.options.video {
            Some(video) => Some((
                peer_connection.new_video_transceiver().await?,
                video.encoder_preferences.clone(),
            )),
            None => None,
        };

        let audio = match whip_ctx.options.audio {
            Some(audio) => Some((
                peer_connection.new_audio_transceiver().await?,
                audio.encoder_preferences.clone(),
            )),
            None => None,
        };

        //let setup_track_before_negotiation = video_encoder_preferences
        //    .as_ref()
        //    .filter(|preferences| preferences.len() == 1)
        //    .and_then(|preferences| {
        //        preferences
        //            .first()
        //            .map(|preference| matches!(preference, VideoEncoderOptions::H264(_)))
        //    })
        //    .unwrap_or(false);

        //let (video_track, video_payload_type, video_encoder_options) =
        //    if setup_track_before_negotiation {
        //        setup_track(
        //            video_transceiver.clone(),
        //            video_encoder_preferences.clone(),
        //            "video".to_string(),
        //        )
        //        .await
        //    } else {
        //        (None, None, None)
        //    };

        //let (audio_track, audio_payload_type, audio_encoder_options) = setup_track(
        //    audio_transceiver.clone(),
        //    audio_encoder_preferences,
        //    "audio".to_string(),
        //)
        //.await;

        let whip_session_url =
            exchange_sdp_offers(peer_connection.clone(), whip_ctx.clone()).await?;

        match audio {
            Some((transceiver, preferences)) => todo!(),
            None => todo!(),
        }

        match video {
            Some((transceiver, preferences)) => todo!(),
            None => todo!(),
        }
//
//        let (audio_track, audio_payload_type, audio_encoder_options) = setup_track(
//            audio_transceiver.clone(),
//            audio_encoder_preferences,
//            "audio".to_string(),
//        )
//        .await;
//
//        let (audio_track, audio_payload_type, audio_encoder_options) = setup_track(
//            audio_transceiver.clone(),
//            audio_encoder_preferences,
//            "audio".to_string(),
//        )
//        .await;

        // let (video_track, video_payload_type, video_encoder_options) = if video_track.is_none() {
        //     setup_track(
        //         video_transceiver.clone(),
        //         video_encoder_preferences,
        //         "video".to_string(),
        //     )
        //     .await
        // } else {
        //     (video_track, video_payload_type, video_encoder_options)
        // };

        // if video_encoder_options.is_none() && whip_ctx.options.video.is_some() {
        //     return Err(WhipError::NoVideoCodecNegotiated);
        // }
        // if audio_encoder_options.is_none() && whip_ctx.options.audio.is_some() {
        //     return Err(WhipError::NoAudioCodecNegotiated);
        // }

        // let (encoder, packet_stream) = create_encoder_and_packet_stream(
        //     whip_ctx.clone(),
        //     video_encoder_options,
        //     video_payload_type,
        //     audio_encoder_options,
        //     audio_payload_type,
        // )?;

        Ok(
            (Self {
                session_url,
                ctx: whip_ctx.pipeline_ctx.clone(),
                client: whip_ctx.client.clone(),
                output_id: whip_ctx.output_id.clone(),
                video,
                audio,
            }),
            WhipSenderTaskHandle {
                video: todo!(),
                audio: todo!(),
            },
        )
    }

    async fn run(self) {
        let video_handle = self.video.map(|(mut video_receiver, video_track)| {
            tokio::spawn(async move {
                loop {
                    match video_receiver.recv().await {
                        Some(PipelineEvent::Data(packet)) => {
                            if let Err(err) = video_track.write_rtp(&packet).await {
                                return Err(err);
                            }
                        }
                        Some(PipelineEvent::EOS) | None => return Ok(()),
                    }
                }
            })
        });

        let audio_handle = self.audio.map(|(mut audio_receiver, audio_track)| {
            tokio::spawn(async move {
                loop {
                    match audio_receiver.recv().await {
                        Some(PipelineEvent::Data(packet)) => {
                            if let Err(err) = audio_track.write_rtp(&packet).await {
                                return Err(err);
                            }
                        }
                        Some(PipelineEvent::EOS) | None => return Ok(()),
                    }
                }
            })
        });

        if let Some(video_handle) = video_handle {
            if let Err(err) = video_handle.await {
                error!("Error occurred while writing to video track, closing connection. {err}");
            }
        }

        if let Some(audio_handle) = audio_handle {
            if let Err(err) = audio_handle.await {
                error!("Error occurred while writing to audio track, closing connection. {err}");
            }
        }

        self.client.delete_session(self.session_url);
        self.ctx
            .event_emitter
            .emit(Event::OutputDone(self.output_id));
        debug!("Closing WHIP sender thread.")
    }
}

async fn handle_keyframe_requests(
    whip_ctx: WhipCtx,
    sender: Arc<RTCRtpSender>,
    keyframe_sender: Sender<()>,
) {
    whip_ctx.pipeline_ctx.tokio_rt.spawn(async move {
        loop {
            if let Ok((packets, _)) = sender.read_rtcp().await {
                for packet in packets {
                    if packet
                        .as_any()
                        .downcast_ref::<PictureLossIndication>()
                        .is_some()
                    {
                        if let Err(err) = keyframe_sender.send(()) {
                            debug!(%err, "Failed to send keyframe request to the encoder.");
                        };
                    }
                }
            } else {
                debug!("Failed to read RTCP packets from the sender.");
            }
        }
    });
}

#[derive(Debug, thiserror::Error)]
pub enum WhipError {
    #[error("Bad status in WHIP response\nStatus: {0}\nBody: {1}")]
    BadStatus(StatusCode, String),

    #[error("WHIP request failed!\nMethod: {0}\nURL: {1}")]
    RequestFailed(Method, Url),

    #[error(
        "Unable to get location endpoint, check correctness of WHIP endpoint and your Bearer token"
    )]
    MissingLocationHeader,

    #[error("Invalid endpoint URL: {1}")]
    InvalidEndpointUrl(#[source] ParseError, String),

    #[error("Failed to create RTC session description: {0}")]
    RTCSessionDescriptionError(webrtc::Error),

    #[error("Failed to set local description: {0}")]
    LocalDescriptionError(webrtc::Error),

    #[error("Failed to set remote description: {0}")]
    RemoteDescriptionError(webrtc::Error),

    #[error("Failed to parse {0} response body: {1}")]
    BodyParsingError(&'static str, reqwest::Error),

    #[error("Failed to create offer: {0}")]
    OfferCreationError(webrtc::Error),

    #[error(transparent)]
    PeerConnectionInitError(#[from] webrtc::Error),

    #[error("Failed to convert ICE candidate to JSON: {0}")]
    IceCandidateToJsonError(webrtc::Error),

    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    PayloadingError(#[from] PayloadingError),

    #[error("Trickle ICE not supported")]
    TrickleIceNotSupported,

    #[error("Entity Tag missing")]
    EntityTagMissing,

    #[error("Entity Tag non-matching")]
    EntityTagNonMatching,

    #[error("Cannot initialize encoder after WHIP negotiation")]
    CannotInitEncoder,

    #[error("No video codec was negotiated")]
    NoVideoCodecNegotiated,

    #[error("No audio codec was negotiated")]
    NoAudioCodecNegotiated,

    #[error("Codec not supported: {0}")]
    UnsupportedCodec(&'static str),
}
