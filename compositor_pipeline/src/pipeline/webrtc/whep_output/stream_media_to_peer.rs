use std::sync::Arc;

use compositor_render::error::ErrorStack;
use tokio::sync::broadcast;
use tracing::{debug, error, info, trace, warn};
use webrtc::track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter};

use crate::event::Event;
use crate::pipeline::rtp::payloader::Payloader;
use crate::pipeline::webrtc::error::WhepError;
use crate::prelude::*;

pub struct MediaStream {
    pub receiver: broadcast::Receiver<EncodedOutputEvent>,
    pub track: Arc<TrackLocalStaticRTP>,
    pub payloader: Payloader,
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
        warn!(
            video_len=?video_stream.as_mut().map(|s| s.receiver.len()),
            audio_len=?audio_stream.as_mut().map(|s| s.receiver.len()),
            "Queue len"
        );
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

        let event = get_output_encoded_event(
            &video_stream,
            &audio_stream,
            &mut next_video_event,
            &mut next_audio_event,
        );
        match event {
            Ok(EncodedOutputEvent::Data(chunk)) => {
                let stream = match chunk.kind {
                    MediaKind::Video(_) => video_stream.as_mut(),
                    MediaKind::Audio(_) => audio_stream.as_mut(),
                };

                if let Some(stream) = stream {
                    let result =
                        send_chunk_to_peer(chunk, &stream.track, &mut stream.payloader).await;
                    if let Err(err) = result {
                        error!("{}", ErrorStack::new(&err).into_string());
                        break;
                    }
                };
            }
            Ok(EncodedOutputEvent::VideoEOS) => info!("Received video EOS event on WHEP output"),
            Ok(EncodedOutputEvent::AudioEOS) => info!("Received audio EOS event on WHEP output"),
            Err(TryGetEventError::Finished) => break,
            Err(TryGetEventError::Empty) => {}
        }
    }

    ctx.event_emitter.emit(Event::OutputDone(output_id));
    debug!("Closing WHEP sender thread.");
}

async fn send_chunk_to_peer(
    chunk: EncodedOutputChunk,
    track: &Arc<TrackLocalStaticRTP>,
    payloader: &mut Payloader,
) -> Result<(), WhepError> {
    match payloader.payload(chunk) {
        Ok(rtp_packets) => {
            for rtp_packet in rtp_packets {
                trace!(?rtp_packet, "WHEP output sending RTP packet");
                if let Err(err) = track.write_rtp(&rtp_packet.packet).await {
                    return Err(WhepError::RtpWriteError(err));
                }
            }
        }
        Err(err) => {
            return Err(WhepError::PayloadingError(err));
        }
    }
    Ok(())
}

enum TryGetEventError {
    Empty,
    Finished,
}

fn get_output_encoded_event(
    video_stream: &Option<MediaStream>,
    audio_stream: &Option<MediaStream>,
    next_video_event: &mut Option<EncodedOutputEvent>,
    next_audio_event: &mut Option<EncodedOutputEvent>,
) -> Result<EncodedOutputEvent, TryGetEventError> {
    // Handle EOS for video
    if let Some(EncodedOutputEvent::VideoEOS) = next_video_event {
        return next_video_event.take().ok_or(TryGetEventError::Empty);
    }

    // Handle EOS for audio
    if let Some(EncodedOutputEvent::AudioEOS) = next_audio_event {
        return next_audio_event.take().ok_or(TryGetEventError::Empty);
    }

    let video_data = match next_video_event {
        Some(EncodedOutputEvent::Data(chunk)) => Some(chunk),
        _ => None,
    };
    let audio_data = match next_audio_event {
        Some(EncodedOutputEvent::Data(chunk)) => Some(chunk),
        _ => None,
    };

    match (&video_data, &audio_data) {
        // try to wait for both audio and video events to be ready
        (Some(video_chunk), Some(audio_chunk)) => {
            if audio_chunk.pts > video_chunk.pts {
                next_video_event.take().ok_or(TryGetEventError::Empty)
            } else {
                next_audio_event.take().ok_or(TryGetEventError::Empty)
            }
        }
        // read audio if there is no way to get video event
        (None, Some(_)) if video_stream.is_none() => {
            next_audio_event.take().ok_or(TryGetEventError::Empty)
        }
        // read video if there is no way to get audio event
        (Some(_), None) if audio_stream.is_none() => {
            next_video_event.take().ok_or(TryGetEventError::Empty)
        }
        (None, None) => Err(TryGetEventError::Finished),
        // we can't do anything here, but there are still receivers
        // that can return something in the next loop.
        //
        // I don't think this can ever happen
        (_, _) => Err(TryGetEventError::Empty),
    }
}
