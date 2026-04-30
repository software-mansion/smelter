use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use tokio::sync::oneshot;
use tracing::{Instrument, Level, debug, span};
use url::Url;

use crate::{
    AudioChannels,
    pipeline::{
        input::Input,
        rtp::{RtpJitterBufferMode, RtpJitterBufferSharedContext},
        webrtc::{
            http_client::{SdpAnswer, WhipWhepHttpClient},
            peer_connection_recvonly::RecvonlyPeerConnection,
            supported_codec_parameters::opus_codec_params,
            whep_input::{
                WhepTrackContext, listen_for_trickle_candidates::listen_for_trickle_candidates,
                on_track::handle_on_track, resolve_video_preferences::resolve_video_preferences,
            },
        },
    },
    queue::{QueueInput, QueueTrackOffset, QueueTrackOptions},
};

use crate::prelude::*;

const WHEP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

/// WHEP input - connects to a remote WebRTC endpoint via WHEP HTTP client,
/// decodes, and feeds frames/samples into the queue.
///
/// ## Codec negotiation
///
/// This side creates the SDP offer. For H.264 decoders (FFmpeg and Vulkan), we
/// advertise constrained baseline, main, and high profiles with the highest
/// supported level per profile. For FfmpegH264 this is always level 5.1;
/// for VulkanH264 the levels come from the GPU's reported decode capabilities.
/// Offer is sent to the server, answer is applied as remote description.
///
/// ## Timestamps
///
/// - Connection is established during input registration (with a 60s timeout).
/// - PTS of first frame will be synced to `queue_sync_point` Instant
/// - Register track with `QueueTrackOffset::Pts(Duration::ZERO)`
/// - Jitter buffer: `RtpJitterBufferMode::RealTime` produces timestamps already in the correct
///   time frame
/// - No way to reconnect
///
/// ### Unsupported scenarios
/// - If ahead of time processing is enabled, initial registration will happen on pts already
///   processed by the queue, but queue will wait and eventually stream will show up, with
///   the portion at the start cut off.
/// - If other input is required and delays queue by X relative to `queue_sync_point.elapsed()`:
///   - If X is smaller than channel sizes then, this input latency will
///     be artificially increased by X.
///   - If X is larger than channel size then, this input will be intermittently
///     blank and streaming until the other inputs (and queue processing) catch up.
#[derive(Debug)]
pub(crate) struct WhepInput {
    ctx: Arc<PipelineCtx>,
    session_url: Url,
    client: Arc<WhipWhepHttpClient>,
    _peer_connection: RecvonlyPeerConnection,
}

impl WhepInput {
    pub(crate) fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: WhepInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let (init_confirmation_sender, init_confirmation_receiver) = oneshot::channel();

        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::Whep,
        });

        let span = span!(
            Level::INFO,
            "WHEP client task",
            input_id = input_ref.to_string()
        );
        let ctx_clone = ctx.clone();
        ctx.spawn_tracked(
            async {
                let result = init_whep_client(input_ref, ctx_clone, options).await;
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
        self.ctx.spawn_tracked(async move {
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
                Err(err) => return Err(Box::new(err).into()),
            },
            Err(err) => match err {
                oneshot::error::TryRecvError::Closed => {
                    return Err(InputInitError::InternalServerError(
                        "WHEP input thread failed to initialize. Result channel closed",
                    ));
                }
                oneshot::error::TryRecvError::Empty => {}
            },
        };
    }
    result_receiver.close();
    Err(Box::new(WebrtcClientError::Timeout).into())
}

async fn init_whep_client(
    input_ref: Ref<InputId>,
    ctx: Arc<PipelineCtx>,
    options: WhepInputOptions,
) -> Result<(Input, InputInitInfo, QueueInput), WebrtcClientError> {
    let client = WhipWhepHttpClient::new(&options.endpoint_url, &options.bearer_token)?;
    let (video_preferences, video_codecs_params) =
        resolve_video_preferences(&ctx, options.video_preferences)?;

    // WHEP input creates the offer (client side), so use hardcoded audio codec defaults.
    // Our decoder supports only stereo.
    let audio_codecs_params = opus_codec_params(true /* fec_first */, AudioChannels::Stereo);
    let pc = RecvonlyPeerConnection::new(&ctx, &video_codecs_params, &audio_codecs_params).await?;

    let _video_transceiver = pc.new_video_track(&video_codecs_params).await?;
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

    let queue_input = QueueInput::new(&ctx, &input_ref, options.queue_options);
    {
        let input_ref = input_ref.clone();
        let ctx = ctx.clone();
        let buffer = RtpJitterBufferSharedContext::new(
            &ctx,
            RtpJitterBufferMode::RealTime,
            ctx.queue_ctx.sync_point,
        );

        let (mut video_sender, mut audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: true,
            audio: true,
            offset: QueueTrackOffset::Pts(Duration::ZERO),
        });

        pc.on_track(move |track_ctx| {
            let ctx = WhepTrackContext::new(track_ctx, &ctx, &buffer);
            handle_on_track(
                ctx,
                input_ref.clone(),
                video_preferences.clone(),
                &mut video_sender,
                &mut audio_sender,
            );
        });
    }

    Ok((
        Input::Whep(WhepInput {
            ctx,
            session_url,
            client,
            _peer_connection: pc,
        }),
        InputInitInfo::Other,
        queue_input,
    ))
}
