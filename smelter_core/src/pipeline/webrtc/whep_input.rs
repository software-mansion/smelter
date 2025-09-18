use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::pipeline::rtp::{RtpNtpSyncPoint, RtpPacket};
use crate::pipeline::webrtc::peer_connection_recvonly::RecvonlyPeerConnection;
use crate::pipeline::webrtc::whep_input::track_audio_thread::process_audio_track;
use crate::pipeline::webrtc::whep_input::track_video_thread::process_video_track;
use crate::pipeline::webrtc::whep_input::whep_http_client::{SdpAnswer, WhepHttpClient};
use crate::{pipeline::input::Input, queue::QueueDataReceiver};
use crossbeam_channel::{bounded, Sender};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, span, warn, Instrument, Level};
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;

use crate::prelude::*;

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
            "WHEP receiver task",
            input_id = input_id.to_string()
        );
        let rt = ctx.tokio_rt.clone();
        rt.spawn(
            async {
                let result =
                    WhepClientTask::new(ctx, options, input_samples_sender, frame_sender)
                        .await;
                match result {
                    Ok(handle) => {
                        init_confirmation_sender.send(Ok(handle)).unwrap();
                    }
                    Err(err) => init_confirmation_sender.send(Err(err)).unwrap(),
                }
            }
            .instrument(span),
        );

        wait_with_deadline(init_confirmation_receiver, WHEP_INIT_TIMEOUT);
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
) -> Result<T, OutputInitError> {
    let start_time = Instant::now();
    while start_time.elapsed() < timeout {
        thread::sleep(Duration::from_millis(500));

        match result_receiver.try_recv() {
            Ok(result) => match result {
                Ok(handle) => return Ok(handle),
                Err(err) => error!("whep todo"),
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

struct WhepClientTrack {
    sender: mpsc::Sender<RtpPacket>,
    track: Arc<TrackLocalStaticRTP>,
}

struct WhepClientTask {
    ctx: Arc<PipelineCtx>,
    client: Arc<WhepHttpClient>,
    input_id: InputId,
    video_track: Option<WhepClientTrack>,
    audio_track: Option<WhepClientTrack>,
}

impl WhepClientTask {
    async fn new(
        ctx: Arc<PipelineCtx>,
        options: WhepInputOptions,
        input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
        frame_sender: Sender<PipelineEvent<Frame>>,
    ) -> Result<WhepInput, WhepInputError> {
        let client = WhepHttpClient::new(&options)?;
        let pc = RecvonlyPeerConnection::new(&ctx, &vec![options.video.unwrap()])
            .await
            .unwrap();

        let _video_transceiver = pc
            .new_video_track(&vec![VideoDecoderOptions::FfmpegH264])
            .await;
        let _audio_transceiver = pc.new_audio_track().await;

        let answer = exchange_sdp_offers(&pc, &client).await.unwrap();

        pc.set_remote_description(answer).await;
        {
            let sync_point = RtpNtpSyncPoint::new(ctx.queue_sync_point);
            pc.on_track(Box::new(move |track, _, transceiver| {
                debug!(
                    kind=?track.kind(),
                    "on_track called"
                );

                let span = span!(Level::INFO, "WHEP input track", track_type =? track.kind());

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

        Ok(WhepInput)
    }
}

async fn exchange_sdp_offers(
    pc: &RecvonlyPeerConnection,
    client: &Arc<WhepHttpClient>,
) -> Result<RTCSessionDescription, WhepInputError> {
    let offer = pc.create_offer().await.unwrap();
    debug!("SDP offer: {}", offer.sdp);

    let SdpAnswer {
        // session_url: location,
        answer,
    } = client.send_offer(&offer).await?;
    debug!("SDP answer: {}", answer.sdp);

    pc.set_local_description(offer).await.unwrap();

    // listen_for_trickle_candidates(pc, client, location.clone());

    Ok(answer)
}
