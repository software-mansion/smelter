use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use tokio::sync::{mpsc, oneshot};
use tracing::{Instrument, Level, debug, error, info, span, trace, warn};
use url::Url;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::track::track_local::{TrackLocalWriter, track_local_static_rtp::TrackLocalStaticRTP};

use establish_peer_connection::exchange_sdp_offers;
use peer_connection::PeerConnection;
use replace_track_with_negotiated_codec::replace_tracks_with_negotiated_codec;
use setup_track::{setup_audio_track, setup_video_track};
use smelter_render::OutputId;
use track_task_audio::WhipAudioTrackThreadHandle;
use track_task_video::WhipVideoTrackThreadHandle;

use crate::{
    event::Event,
    pipeline::{
        output::{Output, OutputAudio, OutputVideo},
        rtp::RtpPacket,
        webrtc::{
            error::WhipError,
            http_client::WhipWhepHttpClient,
            whip_output::codec_preferences::{
                codec_params_from_preferences, resolve_audio_preferences, resolve_video_preferences,
            },
        },
    },
};

use crate::prelude::*;

mod codec_preferences;
mod establish_peer_connection;
mod peer_connection;
mod replace_track_with_negotiated_codec;
mod setup_track;
mod track_task_audio;
mod track_task_video;

/// WHIP output - pushes media to a remote WHIP server.
///
/// ## Codec negotiation
///
/// This side creates the SDP offer from encoder preferences. For H.264 encoders
/// (FFmpeg and Vulkan), the offer includes constrained baseline 3.1 (for Twitch
/// compatibility) and constrained baseline, main, and high profiles at level
/// 5.1. After receiving the answer, we determine which codec was negotiated and
/// select the matching encoder.
#[derive(Debug)]
pub(crate) struct WhipOutput {
    pub video: Option<WhipVideoTrackThreadHandle>,
    pub audio: Option<WhipAudioTrackThreadHandle>,
}

const WHIP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

impl WhipOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: WhipOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let (init_confirmation_sender, init_confirmation_receiver) = oneshot::channel();

        ctx.stats_sender.send(StatsEvent::NewOutput {
            output_ref: output_ref.clone(),
            kind: OutputProtocolKind::Whip,
        });

        let span = span!(
            Level::INFO,
            "WHIP client task",
            output_id = output_ref.to_string()
        );
        let rt = ctx.tokio_rt.clone();
        rt.spawn(
            async {
                let result = WhipClientTask::new(ctx, output_ref, options).await;
                match result {
                    Ok((task, handle)) => {
                        init_confirmation_sender.send(Ok(handle)).unwrap();
                        task.run().await
                    }
                    Err(err) => init_confirmation_sender.send(Err(err)).unwrap(),
                }
            }
            .instrument(span),
        );

        wait_with_deadline(init_confirmation_receiver, WHIP_INIT_TIMEOUT)
    }
}

struct WhipClientTrack {
    receiver: mpsc::Receiver<RtpPacket>,
    track: Arc<TrackLocalStaticRTP>,
}

struct WhipClientTask {
    session_url: Url,
    ctx: Arc<PipelineCtx>,
    client: Arc<WhipWhepHttpClient>,
    output_ref: Ref<OutputId>,
    video_track: Option<WhipClientTrack>,
    audio_track: Option<WhipClientTrack>,
    should_close: Arc<AtomicBool>,

    #[allow(dead_code)]
    pc: PeerConnection,
}

impl WhipClientTask {
    async fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: WhipOutputOptions,
    ) -> Result<(Self, WhipOutput), WebrtcClientError> {
        let video_preferences = resolve_video_preferences(&ctx, &options)?;
        let audio_preferences = resolve_audio_preferences(&options);

        let codec_params = codec_params_from_preferences(&video_preferences, &audio_preferences);

        let client = WhipWhepHttpClient::new(&options.endpoint_url, &options.bearer_token)?;
        let pc = PeerConnection::new(&ctx, codec_params).await?;

        let should_close = Self::register_connection_state_handler(&pc, &ctx, &output_ref);

        let video_rtc_sender = pc.new_video_track().await?;
        let audio_rtc_sender = pc.new_audio_track().await?;

        let (session_url, answer) = exchange_sdp_offers(&pc, &client).await?;

        // webrtc-rs assigns a codec to the transceiver on creation, so we need to ensure that
        // supported codec is set before set_remote_description https://github.com/webrtc-rs/webrtc/issues/737
        //
        // Final codec resolution is based on RTCRtpSendParameters and happens after set_remote_description call.
        replace_tracks_with_negotiated_codec(&answer, &video_rtc_sender, &audio_rtc_sender).await?;

        pc.set_remote_description(answer).await?;

        let (video_thread_handle, video_track) = match video_preferences {
            Some(encoder_preferences) => {
                let (video_thread_handle, video) =
                    setup_video_track(&ctx, &output_ref, video_rtc_sender, encoder_preferences)
                        .await?;
                (Some(video_thread_handle), Some(video))
            }
            None => (None, None),
        };

        let (audio_thread_handle, audio_track) = match audio_preferences {
            Some(encoder_preferences) => {
                let (audio_thread_handle, audio) = setup_audio_track(
                    &ctx,
                    &output_ref,
                    audio_rtc_sender,
                    pc.downgrade(),
                    encoder_preferences,
                )
                .await?;
                (Some(audio_thread_handle), Some(audio))
            }
            None => (None, None),
        };

        Ok((
            Self {
                session_url,
                ctx: ctx.clone(),
                client,
                output_ref,
                video_track,
                audio_track,
                should_close,
                pc,
            },
            WhipOutput {
                video: video_thread_handle,
                audio: audio_thread_handle,
            },
        ))
    }

    /// Registers a connection state handler on the peer connection. Returns a
    /// flag that is set when the connection fails or closes.
    fn register_connection_state_handler(
        pc: &PeerConnection,
        ctx: &Arc<PipelineCtx>,
        output_ref: &Ref<OutputId>,
    ) -> Arc<AtomicBool> {
        let should_close = Arc::new(AtomicBool::new(false));
        let close_flag = should_close.clone();
        let ctx = ctx.clone();
        let output_ref = output_ref.clone();
        pc.on_connection_state_change(move |state| {
            ctx.stats_sender
                .send(WhipOutputStatsEvent::PeerStateChanged(state).into_event(&output_ref));

            match state {
                RTCPeerConnectionState::Disconnected => {
                    ctx.event_emitter.emit(Event::OutputError {
                        output_id: output_ref.id().clone(),
                        severity: ErrorSeverity::Transient,
                        err: OutputWhipRuntimeError::PeerConnectionDisconnected.into(),
                    });
                }
                RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                    close_flag.store(true, Ordering::Relaxed);
                }
                _ => {}
            }
        });
        should_close
    }

    /// Forward packets from audio/video channels while making sure they
    /// are interleaved according to their timestamps
    async fn run(self) {
        let mut packet_sender = InterleavedPacketSender::new(self.video_track, self.audio_track);
        loop {
            let Some((packet, kind)) = packet_sender.resolve_next_packet().await else {
                break;
            };

            if self.should_close.load(Ordering::Relaxed) {
                debug!("Peer connection disconnected, closing WHIP output.");
                break;
            }

            match packet_sender.send_packet_to_peer(&packet, kind).await {
                Ok(_) => trace!(?packet, ?kind, "RTP packet sent."),
                Err(error @ WhipError::RtpWriteError(_)) => {
                    warn!(%error);
                    break;
                }
                Err(error @ WhipError::UnexpectedVideoPacket) => error!(%error),
                Err(error @ WhipError::UnexpectedAudioPacket) => error!(%error),
            }
        }

        self.client.delete_session(self.session_url).await;
        self.ctx
            .event_emitter
            .emit(Event::OutputDone(self.output_ref.id().clone()));
        debug!("Closing WHIP sender thread.")
    }
}

impl Output for WhipOutput {
    fn audio(&self) -> Option<OutputAudio<'_>> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo<'_>> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::Whip
    }
}

struct InterleavedPacketSender {
    video_track: Option<WhipClientTrack>,
    audio_track: Option<WhipClientTrack>,
    next_video: Option<RtpPacket>,
    next_audio: Option<RtpPacket>,
}

#[derive(Debug, Clone, Copy)]
enum PacketKind {
    Video,
    Audio,
}

impl InterleavedPacketSender {
    fn new(video_track: Option<WhipClientTrack>, audio_track: Option<WhipClientTrack>) -> Self {
        Self {
            video_track,
            audio_track,
            next_video: None,
            next_audio: None,
        }
    }

    async fn resolve_next_packet(&mut self) -> Option<(RtpPacket, PacketKind)> {
        loop {
            let needs_video = self.video_track.is_some() && self.next_video.is_none();
            let needs_audio = self.audio_track.is_some() && self.next_audio.is_none();
            match (needs_video, needs_audio) {
                (true, true) => {
                    tokio::select! {
                        packet = self.video_track.as_mut().unwrap().receiver.recv() => {
                            self.handle_video_read(packet);
                        },
                        packet = self.audio_track.as_mut().unwrap().receiver.recv() => {
                            self.handle_audio_read(packet);
                        }
                    }
                }
                (true, false) => {
                    let packet = self.video_track.as_mut().unwrap().receiver.recv().await;
                    self.handle_video_read(packet);
                }
                (false, true) => {
                    let packet = self.audio_track.as_mut().unwrap().receiver.recv().await;
                    self.handle_audio_read(packet);
                }
                (false, false) => return self.resolve_from_state(),
            }
        }
    }

    fn handle_video_read(&mut self, packet: Option<RtpPacket>) {
        match packet {
            Some(packet) => self.next_video = Some(packet),
            None => {
                info!("Received video EOS.");
                self.video_track = None;
            }
        }
    }

    fn handle_audio_read(&mut self, packet: Option<RtpPacket>) {
        match packet {
            Some(packet) => self.next_audio = Some(packet),
            None => {
                info!("Received audio EOS.");
                self.audio_track = None;
            }
        }
    }

    fn resolve_from_state(&mut self) -> Option<(RtpPacket, PacketKind)> {
        match (&self.next_video, &self.next_audio) {
            (Some(video_packet), Some(audio_packet)) => {
                if video_packet.timestamp < audio_packet.timestamp {
                    self.next_video.take().map(|p| (p, PacketKind::Video))
                } else {
                    self.next_audio.take().map(|p| (p, PacketKind::Audio))
                }
            }
            (Some(_), None) => self.next_video.take().map(|p| (p, PacketKind::Video)),
            (None, Some(_)) => self.next_audio.take().map(|p| (p, PacketKind::Audio)),
            (None, None) => None,
        }
    }

    async fn send_packet_to_peer(
        &mut self,
        packet: &RtpPacket,
        kind: PacketKind,
    ) -> Result<(), WhipError> {
        match kind {
            PacketKind::Video => {
                let Some(video_track) = &self.video_track else {
                    return Err(WhipError::UnexpectedVideoPacket);
                };
                video_track.track.write_rtp(&packet.packet).await?;
            }
            PacketKind::Audio => {
                let Some(audio_track) = &self.audio_track else {
                    return Err(WhipError::UnexpectedAudioPacket);
                };
                audio_track.track.write_rtp(&packet.packet).await?;
            }
        }
        Ok(())
    }
}

fn wait_with_deadline<T>(
    mut result_receiver: oneshot::Receiver<Result<T, WebrtcClientError>>,
    timeout: Duration,
) -> Result<T, OutputInitError> {
    let start_time = Instant::now();
    while start_time.elapsed() < timeout {
        thread::sleep(Duration::from_millis(500));

        match result_receiver.try_recv() {
            Ok(result) => match result {
                Ok(handle) => return Ok(handle),
                Err(err) => return Err(OutputInitError::WhipInitError(err.into())),
            },
            Err(err) => match err {
                oneshot::error::TryRecvError::Closed => {
                    return Err(OutputInitError::UnknownWhipError);
                }
                oneshot::error::TryRecvError::Empty => {}
            },
        };
    }
    result_receiver.close();
    Err(OutputInitError::WhipInitTimeout)
}

struct WhipOutputStatsSender {
    stats_sender: StatsSender,
    output_ref: Ref<OutputId>,
}

impl WhipOutputStatsSender {
    pub fn new(stats_sender: StatsSender, output_ref: Ref<OutputId>) -> Self {
        Self {
            stats_sender,
            output_ref,
        }
    }

    fn bytes_sent_event(&self, size: usize, track_kind: StatsTrackKind) {
        self.stats_sender.send(
            WhipOutputTrackStatsEvent::BytesSent(size).into_event(&self.output_ref, track_kind),
        );
    }
}
