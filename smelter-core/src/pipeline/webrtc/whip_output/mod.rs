use establish_peer_connection::exchange_sdp_offers;
use peer_connection::PeerConnection;
use replace_track_with_negotiated_codec::replace_tracks_with_negotiated_codec;
use setup_track::{setup_audio_track, setup_video_track};
use smelter_render::OutputId;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, warn};
use url::Url;
use webrtc::track::track_local::{TrackLocalWriter, track_local_static_rtp::TrackLocalStaticRTP};

use crate::{
    event::Event,
    pipeline::{
        rtp::RtpPacket,
        webrtc::{
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
mod output;
mod peer_connection;
mod replace_track_with_negotiated_codec;
mod setup_track;
mod track_task_audio;
mod track_task_video;

pub(crate) use output::WhipOutput;

struct WhipClientTrack {
    receiver: mpsc::Receiver<RtpPacket>,
    track: Arc<TrackLocalStaticRTP>,
}

struct WhipClientTask {
    session_url: Url,
    ctx: Arc<PipelineCtx>,
    client: Arc<WhipWhepHttpClient>,
    output_id: OutputId,
    video_track: Option<WhipClientTrack>,
    audio_track: Option<WhipClientTrack>,
}

impl WhipClientTask {
    async fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: WhipOutputOptions,
    ) -> Result<(Self, WhipOutput), WebrtcClientError> {
        let video_preferences = resolve_video_preferences(&ctx, &options)?;
        let audio_preferences = resolve_audio_preferences(&options);

        let codec_params = codec_params_from_preferences(&video_preferences, &audio_preferences);

        let client = WhipWhepHttpClient::new(&options.endpoint_url, &options.bearer_token)?;
        let pc = PeerConnection::new(&ctx, codec_params).await?;

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
                    setup_video_track(&ctx, &output_id, video_rtc_sender, encoder_preferences)
                        .await?;
                (Some(video_thread_handle), Some(video))
            }
            None => (None, None),
        };

        let (audio_thread_handle, audio_track) = match audio_preferences {
            Some(encoder_preferences) => {
                let (audio_thread_handle, audio) = setup_audio_track(
                    &ctx,
                    &output_id,
                    audio_rtc_sender,
                    pc.clone(),
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
                output_id,
                video_track,
                audio_track,
            },
            WhipOutput {
                video: video_thread_handle,
                audio: audio_thread_handle,
            },
        ))
    }

    async fn run(self) {
        let (mut audio_receiver, audio_track) = match self.audio_track {
            Some(WhipClientTrack { receiver, track }) => (Some(receiver), Some(track)),
            None => (None, None),
        };

        let (mut video_receiver, video_track) = match self.video_track {
            Some(WhipClientTrack { receiver, track }) => (Some(receiver), Some(track)),
            None => (None, None),
        };
        let mut next_video_packet = None;
        let mut next_audio_packet = None;

        loop {
            match (
                &next_video_packet,
                &next_audio_packet,
                &mut video_receiver,
                &mut audio_receiver,
            ) {
                (None, None, Some(video_receiver), Some(audio_receiver)) => {
                    tokio::select! {
                        Some(packet) = video_receiver.recv() => {
                            next_video_packet = Some(packet)
                        },
                        Some(packet) = audio_receiver.recv() => {
                            next_audio_packet = Some(packet)
                        },
                        else => break,
                    };
                }
                (_video, None, _video_receiver, audio_receiver @ Some(_)) => {
                    match audio_receiver.as_mut().unwrap().recv().await {
                        Some(packet) => {
                            next_audio_packet = Some(packet);
                        }
                        None => *audio_receiver = None,
                    };
                }
                (None, _, video_receiver @ Some(_), _) => {
                    match video_receiver.as_mut().unwrap().recv().await {
                        Some(packet) => {
                            next_video_packet = Some(packet);
                        }
                        None => *video_receiver = None,
                    };
                }
                (None, None, None, None) => {
                    break;
                }
                (Some(_), Some(_), _, _) => {
                    warn!("Both packets populated, this should not happen.");
                }
                (None, Some(_audio), None, _) => {
                    // no video, but can't read audio at this moment
                }
                (Some(_video), None, _, None) => {
                    // no audio, but can't read video at this moment
                }
            };

            match (&next_video_packet, &next_audio_packet) {
                // try to wait for both audio and video packet to be ready
                (Some(video), Some(audio)) => {
                    if audio.timestamp > video.timestamp {
                        if let (Some(packet), Some(track)) =
                            (next_video_packet.take(), &video_track)
                            && let Err(err) = track.write_rtp(&packet.packet).await
                        {
                            warn!("RTP write error {}", err);
                            break;
                        }
                    } else if let (Some(packet), Some(track)) =
                        (next_audio_packet.take(), &audio_track)
                        && let Err(err) = track.write_rtp(&packet.packet).await
                    {
                        warn!("RTP write error {}", err);
                        break;
                    }
                }
                // read audio if there is not way to get video packet
                (None, Some(_)) if video_receiver.is_none() => {
                    if let (Some(p), Some(track)) = (next_audio_packet.take(), &audio_track)
                        && let Err(err) = track.write_rtp(&p.packet).await
                    {
                        warn!("RTP write error {}", err);
                        break;
                    }
                }
                // read video if there is not way to get audio packet
                (Some(_), None) if audio_receiver.is_none() => {
                    if let (Some(p), Some(track)) = (next_video_packet.take(), &video_track)
                        && let Err(err) = track.write_rtp(&p.packet).await
                    {
                        warn!("RTP write error {}", err);
                        break;
                    }
                }
                (None, None) => break,
                // we can't do anything here, but there are still receivers
                // that can return something in the next loop.
                //
                // I don't think this can ever happen
                (_, _) => (),
            };
        }

        self.client.delete_session(self.session_url).await;
        self.ctx
            .event_emitter
            .emit(Event::OutputDone(self.output_id));
        debug!("Closing WHIP sender thread.")
    }
}
