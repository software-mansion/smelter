use compositor_render::OutputId;
use crossbeam_channel::Sender;
use establish_peer_connection::connect;

use init_encoder_after_negotiation::create_encoder_and_packet_stream;
use init_peer_connection::init_peer_connection;
use payloader::{Payload, PayloadingError};
use reqwest::{Method, StatusCode};
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use setup_track::setup_track;
use std::{
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use tracing::{debug, error, span, Instrument, Level};
use url::{ParseError, Url};
use webrtc::{rtp_transceiver::rtp_sender::RTCRtpSender, track::track_local::TrackLocalWriter};

use crate::{
    error::OutputInitError,
    event::Event,
    pipeline::{
        encoder::{AudioEncoderOptions, Encoder, VideoEncoderOptions},
        PipelineCtx,
    },
};

mod establish_peer_connection;
mod init_encoder_after_negotiation;
mod init_peer_connection;
mod packet_stream;
mod payloader;
mod setup_track;

#[derive(Debug)]
pub struct WhipSender {
    pub connection_options: WhipSenderOptions,
    should_close: Arc<AtomicBool>,
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
    pub endpoint_url: String,
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

const WHIP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

impl WhipSender {
    pub fn new(
        output_id: &OutputId,
        options: WhipSenderOptions,
        pipeline_ctx: Arc<PipelineCtx>,
    ) -> Result<(Self, Encoder), OutputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));
        let (init_confirmation_sender, mut init_confirmation_receiver) =
            oneshot::channel::<Result<Encoder, WhipError>>();

        let whip_ctx = WhipCtx {
            output_id: output_id.clone(),
            options: options.clone(),
            should_close: should_close.clone(),
            pipeline_ctx: pipeline_ctx.clone(),
        };

        pipeline_ctx.tokio_rt.spawn(
            run_whip_sender_task(whip_ctx, init_confirmation_sender).instrument(span!(
                Level::INFO,
                "WHIP sender",
                output_id = output_id.to_string()
            )),
        );

        let start_time = Instant::now();
        while start_time.elapsed() < WHIP_INIT_TIMEOUT {
            thread::sleep(Duration::from_millis(500));

            match init_confirmation_receiver.try_recv() {
                Ok(result) => match result {
                    Ok(encoder) => {
                        return Ok((
                            Self {
                                connection_options: options,
                                should_close,
                            },
                            encoder,
                        ))
                    }
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
        init_confirmation_receiver.close();
        Err(OutputInitError::WhipInitTimeout)
    }
}

impl Drop for WhipSender {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

async fn run_whip_sender_task(
    whip_ctx: WhipCtx,
    init_confirmation_sender: oneshot::Sender<Result<Encoder, WhipError>>,
) {
    let client = Arc::new(reqwest::Client::new());
    let (peer_connection, video_transceiver, audio_transceiver) = match init_peer_connection(
        &whip_ctx,
    )
    .await
    {
        Ok(pc) => pc,
        Err(err) => {
            if let Err(Err(err)) = init_confirmation_sender.send(Err(err)) {
                error!(
                    "Error while initializing whip sender thread, couldn't send message, error: {err:?}"
                );
            }
            return;
        }
    };

    let video_encoder_preferences = whip_ctx
        .options
        .video
        .as_ref()
        .map(|v| v.encoder_preferences.clone());
    let audio_encoder_preferences = whip_ctx
        .options
        .audio
        .as_ref()
        .map(|a| a.encoder_preferences.clone());

    let setup_track_before_negotiation = video_encoder_preferences
        .as_ref()
        .filter(|preferences| preferences.len() == 1)
        .and_then(|preferences| {
            preferences
                .first()
                .map(|preference| matches!(preference, VideoEncoderOptions::H264(_)))
        })
        .unwrap_or(false);

    let (video_track, video_payload_type, video_encoder_options) = if setup_track_before_negotiation
    {
        setup_track(
            video_transceiver.clone(),
            video_encoder_preferences.clone(),
            "video".to_string(),
        )
        .await
    } else {
        (None, None, None)
    };

    let (audio_track, audio_payload_type, audio_encoder_options) = setup_track(
        audio_transceiver.clone(),
        audio_encoder_preferences,
        "audio".to_string(),
    )
    .await;

    let whip_session_url = match connect(peer_connection.clone(), client.clone(), &whip_ctx).await {
        Ok(val) => val,
        Err(err) => {
            if let Err(Err(err)) = init_confirmation_sender.send(Err(err)) {
                error!(
                    "Error while initializing whip sender thread, couldn't send message, error: {err:?}"
                );
            }
            return;
        }
    };
    let (video_track, video_payload_type, video_encoder_options) = if video_track.is_none() {
        setup_track(
            video_transceiver.clone(),
            video_encoder_preferences,
            "video".to_string(),
        )
        .await
    } else {
        (video_track, video_payload_type, video_encoder_options)
    };

    if video_encoder_options.is_none() && whip_ctx.options.video.is_some() {
        if let Err(Err(err)) = init_confirmation_sender.send(Err(WhipError::NoVideoCodecNegotiated))
        {
            error!(
                "Error while initializing whip sender thread, couldn't send message, error: {err:?}"
            );
        }
        return;
    }
    if audio_encoder_options.is_none() && whip_ctx.options.audio.is_some() {
        if let Err(Err(err)) = init_confirmation_sender.send(Err(WhipError::NoAudioCodecNegotiated))
        {
            error!(
                "Error while initializing whip sender thread, couldn't send message, error: {err:?}"
            );
        }
        return;
    }

    let (encoder, packet_stream) = match create_encoder_and_packet_stream(
        whip_ctx.clone(),
        video_encoder_options,
        video_payload_type,
        audio_encoder_options,
        audio_payload_type,
    ) {
        Ok((encoder, packet_stream)) => (encoder, packet_stream),
        Err(err) => {
            if let Err(Err(err)) = init_confirmation_sender.send(Err(err)) {
                error!(
                    "Error while initializing whip sender thread, couldn't send message, error: {err:?}"
                );
            }
            return;
        }
    };

    if let Some(keyframe_sender) = encoder.keyframe_request_sender() {
        if let Some(video_transceiver) = video_transceiver {
            let video_sender = video_transceiver.sender().await;
            handle_keyframe_requests(whip_ctx.clone(), video_sender, keyframe_sender.clone()).await;
        }
        if let Some(audio_transceiver) = audio_transceiver {
            let audio_sender = audio_transceiver.sender().await;
            handle_keyframe_requests(whip_ctx.clone(), audio_sender, keyframe_sender.clone()).await;
        }
    }

    if let Err(Ok(_)) = init_confirmation_sender.send(Ok(encoder)) {
        error!("Whip sender thread initialized successfully, coulnd't send confirmation message.");
        return;
    }

    for chunk in packet_stream {
        if whip_ctx
            .should_close
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            break;
        }
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => {
                error!("Failed to payload a packet: {}", err);
                continue;
            }
        };

        match chunk {
            Payload::Video(video_payload) => {
                match video_track.clone() {
                    Some(video_track) => match video_payload {
                        Ok(video_bytes) => {
                            if video_track.write(&video_bytes).await.is_err() {
                                error!("Error occurred while writing to video track, closing connection.");
                                break;
                            }
                        }
                        Err(err) => {
                            error!("Error while reading video bytes: {err}");
                        }
                    },
                    None => {
                        error!("Video payload detected on output with no video, shutting down");
                        break;
                    }
                }
            }
            Payload::Audio(audio_payload) => {
                match audio_track.clone() {
                    Some(audio_track) => match audio_payload {
                        Ok(audio_bytes) => {
                            if audio_track.write(&audio_bytes).await.is_err() {
                                error!("Error occurred while writing to audio track, closing connection.");
                                break;
                            }
                        }
                        Err(err) => {
                            error!("Error while audio video bytes: {err}");
                        }
                    },
                    None => {
                        error!("Audio payload detected on output with no audio, shutting down");
                        break;
                    }
                }
            }
        }
    }
    if let Err(err) = client.delete(whip_session_url).send().await {
        error!("Error while sending delete whip session request: {}", err);
    }
    whip_ctx
        .pipeline_ctx
        .event_emitter
        .emit(Event::OutputDone(whip_ctx.output_id));
    debug!("Closing WHIP sender thread.")
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
