use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{debug, warn};
use webrtc::track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter};

use crate::event::Event;
use crate::pipeline::rtp::payloader::Payloader;
use crate::prelude::*;

pub struct MediaStream {
    pub receiver: Option<broadcast::Receiver<EncodedOutputEvent>>,
    pub track: Option<Arc<TrackLocalStaticRTP>>,
    pub payloader: Option<Payloader>,
}

pub async fn stream_media_to_peer(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    mut video: MediaStream,
    mut audio: MediaStream,
) {
    let mut next_video_event = None;
    let mut next_audio_event = None;

    loop {
        match (
            &next_video_event,
            &next_audio_event,
            &mut video.receiver,
            &mut audio.receiver,
        ) {
            (None, None, Some(video_receiver), Some(audio_receiver)) => {
                tokio::select! {
                    Ok(event) = video_receiver.recv() => {
                        next_video_event = Some(event)
                    },
                    Ok(event) = audio_receiver.recv() => {
                        next_audio_event = Some(event)
                    },
                    else => break,
                };
            }
            (_video, None, _video_receiver, audio_receiver @ Some(_)) => {
                match audio_receiver.as_mut().unwrap().recv().await {
                    Ok(event) => {
                        next_audio_event = Some(event);
                    }
                    Err(_) => *audio_receiver = None,
                };
            }
            (None, _, video_receiver @ Some(_), _) => {
                match video_receiver.as_mut().unwrap().recv().await {
                    Ok(event) => {
                        next_video_event = Some(event);
                    }
                    Err(_) => *video_receiver = None,
                };
            }
            (None, None, None, None) => {
                break;
            }
            (Some(_), Some(_), _, _) => {
                // Both events populated - will process them below
            }
            (None, Some(_audio), None, _) => {
                // no video, but can't read audio at this moment
            }
            (Some(_video), None, _, None) => {
                // no audio, but can't read video at this moment
            }
        };

        match (&next_video_event, &next_audio_event) {
            // try to wait for both audio and video events to be ready
            (Some(video_event), Some(audio_event)) => {
                if get_event_timestamp(audio_event) > get_event_timestamp(video_event) {
                    if let Some(event) = next_video_event.take() {
                        process_video_event(event, &mut video.payloader, &video.track).await;
                    }
                } else if let Some(event) = next_audio_event.take() {
                    process_audio_event(event, &mut audio.payloader, &audio.track).await;
                }
            }
            // read audio if there is no way to get video event
            (None, Some(_)) if video.receiver.is_none() => {
                if let Some(event) = next_audio_event.take() {
                    process_audio_event(event, &mut audio.payloader, &audio.track).await;
                }
            }
            // read video if there is no way to get audio event
            (Some(_), None) if audio.receiver.is_none() => {
                if let Some(event) = next_video_event.take() {
                    process_video_event(event, &mut video.payloader, &video.track).await;
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

    ctx.event_emitter.emit(Event::OutputDone(output_id));
    debug!("Closing WHEP sender thread.");
}

fn get_event_timestamp(event: &EncodedOutputEvent) -> std::time::Duration {
    match event {
        EncodedOutputEvent::Data(chunk) => chunk.pts,
        _ => std::time::Duration::ZERO,
    }
}

async fn process_video_event(
    event: EncodedOutputEvent,
    payloader: &mut Option<Payloader>,
    track: &Option<Arc<TrackLocalStaticRTP>>,
) {
    match event {
        EncodedOutputEvent::Data(chunk) if matches!(chunk.kind, MediaKind::Video(_)) => {
            if let (Some(payloader), Some(track)) = (payloader, track) {
                match payloader.payload(chunk) {
                    Ok(rtp_packets) => {
                        for rtp_packet in rtp_packets {
                            if let Err(err) = track.write_rtp(&rtp_packet.packet).await {
                                warn!("Failed to write video RTP packet: {}", err);
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        warn!("Failed to payload video chunk: {}", err);
                    }
                }
            }
        }
        _ => {
            // Ignore non-video events or EOS
        }
    }
}

async fn process_audio_event(
    event: EncodedOutputEvent,
    payloader: &mut Option<Payloader>,
    track: &Option<Arc<TrackLocalStaticRTP>>,
) {
    match event {
        EncodedOutputEvent::Data(chunk) if matches!(chunk.kind, MediaKind::Audio(_)) => {
            if let (Some(payloader), Some(track)) = (payloader, track) {
                match payloader.payload(chunk) {
                    Ok(rtp_packets) => {
                        for rtp_packet in rtp_packets {
                            if let Err(err) = track.write_rtp(&rtp_packet.packet).await {
                                warn!("Failed to write audio RTP packet: {}", err);
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        warn!("Failed to payload audio chunk: {}", err);
                    }
                }
            }
        }
        _ => {
            // Ignore non-audio events or EOS
        }
    }
}
