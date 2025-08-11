use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{debug, warn};
use webrtc::track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter};

use crate::{event::Event, pipeline::rtp::RtpPacket};
use crate::prelude::*;

pub async fn stream_media_to_peer(
    ctx: Arc<PipelineCtx>,
    output_id: &OutputId,
    mut video_receiver: Option<broadcast::Receiver<RtpPacket>>,
    mut audio_receiver: Option<broadcast::Receiver<RtpPacket>>,
    video_track: Option<Arc<TrackLocalStaticRTP>>,
    audio_track: Option<Arc<TrackLocalStaticRTP>>,
) {
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
                    Ok(packet) = video_receiver.recv() => {
                        next_video_packet = Some(packet)
                    },
                    Ok(packet) = audio_receiver.recv() => {
                        next_audio_packet = Some(packet)
                    },
                    else => break,
                };
            }
            (_video, None, _video_receiver, audio_receiver @ Some(_)) => {
                match audio_receiver.as_mut().unwrap().recv().await {
                    Ok(packet) => {
                        next_audio_packet = Some(packet);
                    }
                    Err(_) => *audio_receiver = None,
                };
            }
            (None, _, video_receiver @ Some(_), _) => {
                match video_receiver.as_mut().unwrap().recv().await {
                    Ok(packet) => {
                        next_video_packet = Some(packet);
                    }
                    Err(_) => *video_receiver = None,
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
                    if let (Some(packet), Some(track)) = (next_video_packet.take(), &video_track) {
                        if let Err(err) = track.write_rtp(&packet.packet).await {
                            warn!("RTP write error {}", err);
                            break;
                        }
                    }
                } else if let (Some(packet), Some(track)) = (next_audio_packet.take(), &audio_track)
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

    ctx.event_emitter.emit(Event::OutputDone(output_id.clone()));
    debug!("Closing WHEP sender thread.")
}
