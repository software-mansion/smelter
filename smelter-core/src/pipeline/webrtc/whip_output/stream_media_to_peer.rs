use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use smelter_render::{OutputId, error::ErrorStack};
use tracing::{debug, error, info, trace, warn};
use url::Url;
use webrtc::{
    peer_connection::peer_connection_state::RTCPeerConnectionState,
    track::track_local::TrackLocalWriter,
};

use crate::{
    PipelineCtx, Ref,
    error::{ErrorSeverity, OutputWhipRuntimeError},
    event::Event,
    pipeline::{
        rtp::RtpPacket,
        webrtc::{
            WhipOutput,
            error::WhipError,
            http_client::WhipWhepHttpClient,
            whip_output::{
                codec_preferences::{
                    codec_params_from_preferences, resolve_audio_preferences,
                    resolve_video_preferences,
                },
                establish_peer_connection::exchange_sdp_offers,
                output::WhipClientTrack,
                peer_connection::PeerConnection,
                replace_track_with_negotiated_codec::replace_tracks_with_negotiated_codec,
                setup_track::{setup_audio_track, setup_video_track},
            },
        },
    },
    prelude::{WebrtcClientError, WhipOutputOptions},
    stats::WhipOutputStatsEvent,
};

pub(super) struct WhipClientTask {
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
    pub async fn new(
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
    pub async fn run(self) {
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
                Err(err) => {
                    warn!("{}", ErrorStack::new(&err).into_string());
                    break;
                }
            }
        }

        self.client.delete_session(self.session_url).await;
        self.ctx
            .event_emitter
            .emit(Event::OutputDone(self.output_ref.id().clone()));
        debug!("Closing WHIP sender thread.")
    }
}

pub(super) struct InterleavedPacketSender {
    video_track: Option<WhipClientTrack>,
    audio_track: Option<WhipClientTrack>,
    next_video: Option<RtpPacket>,
    next_audio: Option<RtpPacket>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum PacketKind {
    Video,
    Audio,
}

impl InterleavedPacketSender {
    pub fn new(video_track: Option<WhipClientTrack>, audio_track: Option<WhipClientTrack>) -> Self {
        Self {
            video_track,
            audio_track,
            next_video: None,
            next_audio: None,
        }
    }

    pub async fn resolve_next_packet(&mut self) -> Option<(RtpPacket, PacketKind)> {
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

    pub async fn send_packet_to_peer(
        &mut self,
        packet: &RtpPacket,
        kind: PacketKind,
    ) -> Result<(), WhipError> {
        match kind {
            PacketKind::Video => {
                let Some(video_track) = &self.video_track else {
                    error!("Received unexpected video packet.");
                    return Ok(());
                };
                video_track.track.write_rtp(&packet.packet).await?;
            }
            PacketKind::Audio => {
                let Some(audio_track) = &self.audio_track else {
                    error!("Received unexpected audio packet.");
                    return Ok(());
                };
                audio_track.track.write_rtp(&packet.packet).await?;
            }
        }
        Ok(())
    }
}
