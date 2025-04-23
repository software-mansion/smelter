use compositor_render::{OutputId, Resolution};
use crossbeam_channel::Sender;
use establish_peer_connection::connect;

use init_peer_connection::init_peer_connection;
use packet_stream::PacketStream;
use payloader::{
    AudioPayloaderOptions, Payload, Payloader, PayloadingError, VideoPayloaderOptions,
};
use reqwest::{Method, StatusCode};
use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use std::{
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use tracing::{debug, error, span, Instrument, Level};
use url::{ParseError, Url};
use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8},
    rtp_transceiver::{
        rtp_codec::RTCRtpCodecCapability, rtp_sender::RTCRtpSender, PayloadType, RTCRtpTransceiver,
    },
    track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter},
};

use crate::{
    audio_mixer::AudioChannels,
    error::OutputInitError,
    event::Event,
    pipeline::{
        encoder::{
            ffmpeg_h264::{self, EncoderPreset},
            ffmpeg_vp8,
            opus::OpusEncoderOptions,
            AudioEncoderOptions, AudioEncoderPreset, Encoder, EncoderOptions, VideoEncoderOptions,
        },
        PipelineCtx,
    },
};

mod establish_peer_connection;
mod init_peer_connection;
mod packet_stream;
mod payloader;

#[derive(Debug)]
pub struct WhipSender {
    pub connection_options: WhipSenderOptions,
    should_close: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct VideoWhipOptions {
    pub resolution: Resolution,
}

#[derive(Debug, Clone)]
pub struct AudioWhipOptions;

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
    let (video_track, video_codec, video_payload_type) =
        setup_track(video_transceiver.clone(), "video".to_string()).await;
    let (audio_track, audio_codec, audio_payload_type) =
        setup_track(audio_transceiver.clone(), "audio".to_string()).await;

    println!(
        "Payload types: {:?}, {:?}",
        video_payload_type, audio_payload_type
    );

    let (encoder, packet_stream) = match create_encoder_and_packet_stream(
        whip_ctx.clone(),
        video_codec,
        video_payload_type,
        audio_codec,
        audio_payload_type,
    ) {
        Ok((encoder, packet_stream)) => (encoder, packet_stream),
        Err(err) => {
            error!("Error message: {:?}", err);
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

fn create_encoder_and_packet_stream(
    whip_ctx: WhipCtx,
    video_codec: Option<RTCRtpCodecCapability>,
    video_payload_type: Option<PayloadType>,
    audio_codec: Option<RTCRtpCodecCapability>,
    audio_payload_type: Option<PayloadType>,
) -> Result<(Encoder, PacketStream), WhipError> {
    let video_encoder_options = if let Some(video_config) = whip_ctx.options.video {
        let resolution = video_config.resolution;
        match video_codec.as_ref().map(|vc| vc.mime_type.as_str()) {
            Some(MIME_TYPE_H264) => Some(VideoEncoderOptions::H264(ffmpeg_h264::Options {
                preset: EncoderPreset::Fast,
                resolution,
                raw_options: vec![],
            })),
            Some(MIME_TYPE_VP8) => Some(VideoEncoderOptions::VP8(ffmpeg_vp8::Options {
                resolution,
                raw_options: vec![],
            })),
            Some(_) | None => None,
        }
    } else {
        None
    };

    let audio_encoder_options = if let Some(_audio_config) = whip_ctx.options.audio {
        //TODO get audio codec preferences from audio_config
        match audio_codec.as_ref().map(|ac| ac.mime_type.as_str()) {
            Some(MIME_TYPE_OPUS) => Some(AudioEncoderOptions::Opus(OpusEncoderOptions {
                channels: AudioChannels::Stereo,
                preset: AudioEncoderPreset::Quality,
                sample_rate: 48000,
            })),
            Some(_) | None => None,
        }
    } else {
        None
    };

    let Ok((encoder, packets_receiver)) = Encoder::new(
        &whip_ctx.output_id,
        EncoderOptions {
            video: video_encoder_options.clone(),
            audio: audio_encoder_options.clone(),
        },
        &whip_ctx.pipeline_ctx,
    ) else {
        return Err(WhipError::CannotInitEncoder);
    };

    let video_payloader_options = Some(VideoPayloaderOptions {
        encoder_options: video_encoder_options.unwrap(),
        payload_type: video_payload_type.unwrap(),
    });
    let audio_payloader_options = Some(AudioPayloaderOptions {
        encoder_options: audio_encoder_options.unwrap(),
        payload_type: audio_payload_type.unwrap(),
    });

    let payloader = Payloader::new(video_payloader_options, audio_payloader_options);
    let packet_stream = PacketStream::new(packets_receiver, payloader, 1400);

    Ok((encoder, packet_stream))
}

async fn setup_track(
    transceiver: Option<Arc<RTCRtpTransceiver>>,
    track_kind: String,
) -> (
    Option<Arc<TrackLocalStaticRTP>>,
    Option<RTCRtpCodecCapability>,
    Option<PayloadType>,
) {
    if let Some(transceiver) = transceiver {
        let sender = transceiver.sender().await;
        let (track, codec, payload_type) =
            match sender.get_parameters().await.rtp_parameters.codecs.first() {
                Some(codec_parameters) => {
                    let track = Arc::new(TrackLocalStaticRTP::new(
                        codec_parameters.capability.clone(),
                        track_kind.clone(),
                        "webrtc-rs".to_string(),
                    ));
                    if let Err(e) = sender.replace_track(Some(track.clone())).await {
                        error!("Failed to replace {} track: {}", track_kind, e);
                    }
                    (
                        Some(track),
                        Some(codec_parameters.capability.clone()),
                        Some(codec_parameters.payload_type),
                    )
                }
                None => (None, None, None),
            };
        (track, codec, payload_type)
    } else {
        (None, None, None)
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
