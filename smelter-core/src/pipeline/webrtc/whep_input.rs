use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::pipeline::webrtc::whep_input::process_tracks::setup_track_processing;
use crate::pipeline::{
    input::Input,
    webrtc::{
        http_client::{SdpAnswer, WhipWhepHttpClient},
        peer_connection_recvonly::RecvonlyPeerConnection,
        whep_input::{
            listen_for_trickle_candidates::listen_for_trickle_candidates,
            resolve_video_preferences::resolve_video_preferences,
        },
    },
};
use crate::queue::QueueDataReceiver;
use crossbeam_channel::bounded;
use tokio::sync::oneshot;
use tracing::{Instrument, Level, debug, span};
use url::Url;

use crate::prelude::*;

mod listen_for_trickle_candidates;
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

        let span = span!(
            Level::INFO,
            "WHEP client task",
            input_id = input_id.to_string()
        );
        let ctx_clone = ctx.clone();
        ctx.tokio_rt.spawn(
            async {
                let result = init_whep_client(ctx_clone, options).await;
                match result {
                    Ok(handle) => init_confirmation_sender.send(Ok(handle)),
                    Err(err) => init_confirmation_sender.send(Err(err)),
                }
            }
            .instrument(span),
        );

        wait_with_deadline(init_confirmation_receiver, WHEP_INIT_TIMEOUT)
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

fn wait_with_deadline<T>(
    mut result_receiver: oneshot::Receiver<Result<T, WebrtcClientError>>,
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
) -> Result<(Input, InputInitInfo, QueueDataReceiver), WebrtcClientError> {
    let (frame_sender, frame_receiver) = bounded(5);
    let (input_samples_sender, input_samples_receiver) = bounded(5);

    let client = WhipWhepHttpClient::new(&options.endpoint_url, &options.bearer_token)?;
    let (video_preferences, video_codecs_params) =
        resolve_video_preferences(&ctx, options.video_preferences)?;
    let pc = RecvonlyPeerConnection::new(&ctx, &video_codecs_params).await?;

    let _video_transceiver = pc.new_video_track(video_codecs_params).await?;
    let _audio_transceiver = pc.new_audio_track().await?;

    let offer = pc.create_offer().await?;
    debug!("SDP offer: {}", offer.sdp);

    let SdpAnswer {
        session_url,
        answer,
    } = client.send_offer(&offer).await?;
    debug!("SDP answer: {}", answer.sdp);

    pc.set_local_description(offer).await?;

    listen_for_trickle_candidates(&pc, &client, session_url.clone());

    pc.set_remote_description(answer).await?;

    setup_track_processing(
        &pc,
        &ctx,
        input_samples_sender,
        frame_sender,
        video_preferences,
    );

    Ok((
        Input::Whep(WhepInput {
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
