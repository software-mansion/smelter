use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::pipeline::{
    input::Input,
    rtp::RtpNtpSyncPoint,
    webrtc::{
        http_client::WhipWhepHttpClient,
        peer_connection_recvonly::RecvonlyPeerConnection,
        whep_input::{
            establish_peer_connection::exchange_sdp_offers,
            process_tracks::{process_audio_track, process_video_track},
            resolve_video_preferences::resolve_video_preferences,
        },
    },
};
use crate::queue::QueueDataReceiver;
use crossbeam_channel::{Sender, bounded};
use tokio::sync::oneshot;
use tracing::{Instrument, Level, debug, span, warn};
use url::Url;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;

use crate::prelude::*;

mod establish_peer_connection;
mod process_tracks;
mod resolve_video_preferences;

const WHEP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct WhepInput {
    ctx: Arc<PipelineCtx>,
    session_url: Url,
    client: Arc<WhipWhepHttpClient>,
}

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
        let ctx_clone = ctx.clone();
        rt.spawn(
            async {
                let result =
                    init_whep_client(ctx_clone, options, input_samples_sender, frame_sender).await;
                match result {
                    Ok(handle) => {
                        init_confirmation_sender.send(Ok(handle)).unwrap();
                    }
                    Err(err) => init_confirmation_sender.send(Err(err)).unwrap(),
                }
            }
            .instrument(span),
        );

        let (session_url, client) =
            wait_with_deadline(init_confirmation_receiver, WHEP_INIT_TIMEOUT)?;
        Ok((
            Input::Whep(Self {
                ctx,
                session_url,
                client,
            }),
            InputInitInfo::Other,
            QueueDataReceiver {
                video: Some(frame_receiver),
                audio: Some(input_samples_receiver),
            },
        ))
    }
}

impl Drop for WhepInput {
    fn drop(&mut self) {
        let session_url = self.session_url.clone();
        let client = self.client.clone();
        self.ctx.tokio_rt.spawn(async move {
            client.delete_session(session_url).await;
        });
    }
}

fn wait_with_deadline(
    mut result_receiver: oneshot::Receiver<
        Result<(Url, Arc<WhipWhepHttpClient>), WebrtcClientError>,
    >,
    timeout: Duration,
) -> Result<(Url, Arc<WhipWhepHttpClient>), InputInitError> {
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
                    return Err(InputInitError::UnknownWhepError);
                }
                oneshot::error::TryRecvError::Empty => {}
            },
        };
    }
    result_receiver.close();
    Err(InputInitError::WhepInitTimeout)
}

async fn init_whep_client(
    ctx: Arc<PipelineCtx>,
    options: WhepInputOptions,
    input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
) -> Result<(Url, Arc<WhipWhepHttpClient>), WebrtcClientError> {
    let client = WhipWhepHttpClient::new(&options.endpoint_url, &options.bearer_token)?;
    let (video_preferences, video_codecs_params) =
        resolve_video_preferences(&ctx, options.video_preferences)?;
    let pc = RecvonlyPeerConnection::new(&ctx, &video_codecs_params).await?;

    let _video_transceiver = pc.new_video_track(video_codecs_params).await?;
    let _audio_transceiver = pc.new_audio_track().await?;

    let (session_url, answer) = exchange_sdp_offers(&pc, &client).await?;
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

    Ok((session_url, client))
}
