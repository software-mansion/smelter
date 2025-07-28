use compositor_render::OutputId;
use establish_peer_connection::exchange_sdp_offers;

use peer_connection::PeerConnection;
use setup_track::{setup_audio_track, setup_video_track};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, span, warn, Instrument, Level};
use track_task_audio::WhipAudioTrackThreadHandle;
use track_task_video::WhipVideoTrackThreadHandle;
use url::Url;
use webrtc::track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter};
use whip_http_client::WhipHttpClient;

use crate::{
    event::Event,
    pipeline::{
        output::{Output, OutputAudio, OutputVideo},
        rtp::RtpPacket,
    },
};

use crate::prelude::*;

mod establish_peer_connection;
mod setup_track;

mod peer_connection;
mod track_task_audio;
mod track_task_video;
mod whip_http_client;

#[derive(Debug)]
pub(crate) struct WhipOutput {
    pub video: Option<WhipVideoTrackThreadHandle>,
    pub audio: Option<WhipAudioTrackThreadHandle>,
}

const WHIP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

impl WhipOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: WhipSenderOptions,
    ) -> Result<Self, OutputInitError> {
        let (init_confirmation_sender, init_confirmation_receiver) = oneshot::channel();

        let span = span!(
            Level::INFO,
            "WHIP sender task",
            output_id = output_id.to_string()
        );
        let rt = ctx.tokio_rt.clone();
        rt.spawn(
            async {
                let result = WhipClientTask::new(ctx, output_id, options).await;
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

struct WhipSenderTrack {
    receiver: mpsc::Receiver<RtpPacket>,
    track: Arc<TrackLocalStaticRTP>,
}

struct WhipClientTask {
    session_url: Url,
    ctx: Arc<PipelineCtx>,
    client: Arc<WhipHttpClient>,
    output_id: OutputId,
    video_track: Option<WhipSenderTrack>,
    audio_track: Option<WhipSenderTrack>,
}

impl WhipClientTask {
    async fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: WhipSenderOptions,
    ) -> Result<(Self, WhipOutput), WhipInputError> {
        let client = WhipHttpClient::new(&options)?;
        let pc = PeerConnection::new(&ctx, &options).await?;

        let video_rtc_sender = pc.new_video_track().await?;
        let audio_rtc_sender = pc.new_audio_track().await?;

        let (session_url, answer) = exchange_sdp_offers(&pc, &client).await?;

        // disable tracks before set remote description
        video_rtc_sender.replace_track(None).await?;
        audio_rtc_sender.replace_track(None).await?;

        pc.set_remote_description(answer).await?;

        let (video_thread_handle, video_track) = match &options.video {
            Some(opts) => {
                let (video_thread_handle, video) =
                    setup_video_track(&ctx, &output_id, video_rtc_sender, opts).await?;
                (Some(video_thread_handle), Some(video))
            }
            None => (None, None),
        };

        let (audio_thread_handle, audio_track) = match &options.audio {
            Some(opts) => {
                let (audio_thread_handle, audio) =
                    setup_audio_track(&ctx, &output_id, audio_rtc_sender, pc.clone(), opts).await?;
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
            Some(WhipSenderTrack { receiver, track }) => (Some(receiver), Some(track)),
            None => (None, None),
        };

        let (mut video_receiver, video_track) = match self.video_track {
            Some(WhipSenderTrack { receiver, track }) => (Some(receiver), Some(track)),
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
                        {
                            if let Err(err) = track.write_rtp(&packet.packet).await {
                                warn!("RTP write error {}", err);
                                break;
                            }
                        }
                    } else if let (Some(packet), Some(track)) =
                        (next_audio_packet.take(), &audio_track)
                    {
                        if let Err(err) = track.write_rtp(&packet.packet).await {
                            warn!("RTP write error {}", err);
                            break;
                        }
                    }
                }
                // read audio if there is not way to get video packet
                (None, Some(_)) if video_receiver.is_none() => {
                    if let (Some(p), Some(track)) = (next_audio_packet.take(), &audio_track) {
                        if let Err(err) = track.write_rtp(&p.packet).await {
                            warn!("RTP write error {}", err);
                            break;
                        }
                    }
                }
                // read video if there is not way to get audio packet
                (Some(_), None) if audio_receiver.is_none() => {
                    if let (Some(p), Some(track)) = (next_video_packet.take(), &video_track) {
                        if let Err(err) = track.write_rtp(&p.packet).await {
                            warn!("RTP write error {}", err);
                            break;
                        }
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

impl Output for WhipOutput {
    fn audio(&self) -> Option<OutputAudio> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo> {
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

fn wait_with_deadline<T>(
    mut result_receiver: oneshot::Receiver<Result<T, WhipInputError>>,
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
                    return Err(OutputInitError::UnknownWhipError)
                }
                oneshot::error::TryRecvError::Empty => {}
            },
        };
    }
    result_receiver.close();
    Err(OutputInitError::WhipInitTimeout)
}
