use std::sync::Arc;

use anyhow::Error;
use tokio::sync::broadcast;
use tracing::{debug, warn};
use webrtc::track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter};

use crate::event::Event;
use crate::pipeline::rtp::payloader::Payloader;
use crate::prelude::*;

pub struct MediaStream {
    pub receiver: broadcast::Receiver<EncodedOutputEvent>,
    pub track: Arc<TrackLocalStaticRTP>,
    pub payloader: Payloader,
}

enum OutputState {
    Ready(EncodedOutputEvent),
    Pending,
    Done,
}

impl From<Option<EncodedOutputEvent>> for OutputState {
    fn from(opt: Option<EncodedOutputEvent>) -> Self {
        match opt {
            Some(v) => OutputState::Ready(v),
            None => OutputState::Pending,
        }
    }
}

pub async fn stream_media_to_peer(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    mut video_stream: Option<MediaStream>,
    mut audio_stream: Option<MediaStream>,
) {
    let mut next_video_event = None;
    let mut next_audio_event = None;

    loop {
        match (
            &next_video_event,
            &next_audio_event,
            &mut video_stream,
            &mut audio_stream,
        ) {
            (None, None, Some(video_stream), Some(audio_stream)) => {
                tokio::select! {
                    Ok(event) = video_stream.receiver.recv() => {
                        next_video_event = Some(event)
                    },
                    Ok(event) = audio_stream.receiver.recv() => {
                        next_audio_event = Some(event)
                    },
                    else => break,
                };
            }
            (_video, None, _video_stream, audio_stream @ Some(_)) => {
                match audio_stream.as_mut().unwrap().receiver.recv().await {
                    Ok(event) => {
                        next_audio_event = Some(event);
                    }
                    Err(_) => *audio_stream = None,
                };
            }
            (None, _, video_stream @ Some(_), _) => {
                match video_stream.as_mut().unwrap().receiver.recv().await {
                    Ok(event) => {
                        next_video_event = Some(event);
                    }
                    Err(_) => *video_stream = None,
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

        match get_output_encoded_event(
            &video_stream,
            &audio_stream,
            &mut next_video_event,
            &mut next_audio_event,
        ) {
            OutputState::Ready(EncodedOutputEvent::Data(chunk)) => {
                if process_chunk(chunk, &mut video_stream, &mut audio_stream)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            OutputState::Ready(_) => {
                // Ignore EOS events
            }
            OutputState::Done => break,
            OutputState::Pending => {}
        }
    }

    ctx.event_emitter.emit(Event::OutputDone(output_id));
    debug!("Closing WHEP sender thread.");
}

async fn process_chunk(
    chunk: EncodedOutputChunk,
    video_stream: &mut Option<MediaStream>,
    audio_stream: &mut Option<MediaStream>,
) -> Result<(), Error> {
    let (stream, kind_label) = match chunk.kind {
        MediaKind::Video(_) => (video_stream.as_mut(), "video"),
        MediaKind::Audio(_) => (audio_stream.as_mut(), "audio"),
    };

    if let Some(MediaStream {
        track, payloader, ..
    }) = stream
    {
        send_chunk_to_peer(chunk, track, payloader, kind_label).await
    } else {
        Ok(())
    }
}

async fn send_chunk_to_peer(
    chunk: EncodedOutputChunk,
    track: &Arc<TrackLocalStaticRTP>,
    payloader: &mut Payloader,
    label: &str,
) -> Result<(), Error> {
    match payloader.payload(chunk) {
        Ok(rtp_packets) => {
            for rtp_packet in rtp_packets {
                if let Err(err) = track.write_rtp(&rtp_packet.packet).await {
                    warn!("Failed to write {} RTP packet: {}", label, err);
                    return Err(err.into());
                }
            }
        }
        Err(err) => {
            warn!("Failed to payload {} chunk: {}", label, err);
            return Err(err.into());
        }
    }
    Ok(())
}

fn get_output_encoded_event(
    video_stream: &Option<MediaStream>,
    audio_stream: &Option<MediaStream>,
    next_video_event: &mut Option<EncodedOutputEvent>,
    next_audio_event: &mut Option<EncodedOutputEvent>,
) -> OutputState {
    match (&next_video_event, &next_audio_event) {
        // try to wait for both audio and video events to be ready
        (Some(video_event), Some(audio_event)) => {
            if get_event_timestamp(audio_event) > get_event_timestamp(video_event) {
                next_video_event.take().into()
            } else {
                next_audio_event.take().into()
            }
        }
        // read audio if there is no way to get video event
        (None, Some(_)) if video_stream.is_none() => next_audio_event.take().into(),
        // read video if there is no way to get audio event
        (Some(_), None) if audio_stream.is_none() => next_video_event.take().into(),
        (None, None) => OutputState::Done,
        // we can't do anything here, but there are still receivers
        // that can return something in the next loop.
        //
        // I don't think this can ever happen
        (_, _) => OutputState::Pending,
    }
}

fn get_event_timestamp(event: &EncodedOutputEvent) -> std::time::Duration {
    match event {
        EncodedOutputEvent::Data(chunk) => chunk.pts,
        _ => std::time::Duration::ZERO,
    }
}
