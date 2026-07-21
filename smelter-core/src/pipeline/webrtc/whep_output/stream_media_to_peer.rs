use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use smelter_render::error::ErrorStack;
use tokio::sync::broadcast;
use tracing::{error, info, trace};
use webrtc::track::track_local::{TrackLocalWriter, track_local_static_rtp::TrackLocalStaticRTP};

use crate::pipeline::{rtp::payloader::Payloader, webrtc::error::WhepError};

use crate::prelude::*;

pub(super) struct MediaStreamTask {
    sender: InterleavedPacketSender,
    should_close: Arc<AtomicBool>,
}

impl MediaStreamTask {
    pub fn new(
        video_stream: Option<MediaStream>,
        audio_stream: Option<MediaStream>,
        should_close: Arc<AtomicBool>,
    ) -> Self {
        let sender = InterleavedPacketSender {
            video_stream,
            audio_stream,
            next_video: None,
            next_audio: None,
        };
        Self {
            sender,
            should_close,
        }
    }

    pub fn spawn(self) {
        tokio::spawn(self.run());
    }

    async fn run(mut self) {
        loop {
            let event = match self.sender.resolve_next_event().await {
                Ok(event) => event,
                Err(InterleaveResult::Abort) => break,
                Err(InterleaveResult::Continue) => continue,
            };

            if self.should_close.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            match event {
                EncodedOutputEvent::Data(chunk) => {
                    if let Err(err) = self.sender.send_chunk_to_peer(chunk).await {
                        error!("{}", ErrorStack::new(&err).into_string());
                        break;
                    }
                }
                EncodedOutputEvent::VideoEOS => info!("Received video EOS event on WHEP output"),
                EncodedOutputEvent::AudioEOS => info!("Received audio EOS event on WHEP output"),
            }
        }
    }
}

pub struct MediaStream {
    pub receiver: broadcast::Receiver<EncodedOutputEvent>,
    pub track: Arc<TrackLocalStaticRTP>,
    pub payloader: Payloader,
}

struct InterleavedPacketSender {
    video_stream: Option<MediaStream>,
    audio_stream: Option<MediaStream>,
    next_video: Option<EncodedOutputEvent>,
    next_audio: Option<EncodedOutputEvent>,
}

#[derive(Debug, PartialEq)]
enum InterleaveResult {
    Continue,
    Abort,
}

impl InterleavedPacketSender {
    async fn resolve_next_event(&mut self) -> Result<EncodedOutputEvent, InterleaveResult> {
        if self.populate_from_channel().await == InterleaveResult::Abort {
            return Err(InterleaveResult::Abort);
        }
        self.resolve_from_state()
    }

    // Read data from audio and video channel to populate next_ fields.
    async fn populate_from_channel(&mut self) -> InterleaveResult {
        match (
            &mut self.next_video,
            &mut self.next_audio,
            &mut self.video_stream,
            &mut self.audio_stream,
        ) {
            (None, None, Some(video_stream), Some(audio_stream)) => {
                tokio::select! {
                    Ok(event) = video_stream.receiver.recv() => {
                        self.next_video = Some(event)
                    },
                    Ok(event) = audio_stream.receiver.recv() => {
                        self.next_audio = Some(event)
                    },
                    else => return InterleaveResult::Abort,
                };
            }
            (_video, None, _video_stream, audio_stream @ Some(_)) => {
                match audio_stream.as_mut().unwrap().receiver.recv().await {
                    Ok(event) => {
                        self.next_audio = Some(event);
                    }
                    Err(_) => *audio_stream = None,
                };
            }
            (None, _audio, video_stream @ Some(_), _audio_stream) => {
                match video_stream.as_mut().unwrap().receiver.recv().await {
                    Ok(event) => {
                        self.next_video = Some(event);
                    }
                    Err(_) => *video_stream = None,
                };
            }
            (None, None, None, None) => return InterleaveResult::Abort,
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

        InterleaveResult::Continue
    }

    fn resolve_from_state(&mut self) -> Result<EncodedOutputEvent, InterleaveResult> {
        // Handle EOS for video
        if let Some(EncodedOutputEvent::VideoEOS) = self.next_video {
            return self.next_video.take().ok_or(InterleaveResult::Continue);
        }

        // Handle EOS for audio
        if let Some(EncodedOutputEvent::AudioEOS) = self.next_audio {
            return self.next_audio.take().ok_or(InterleaveResult::Continue);
        }

        let video_data = match &self.next_video {
            Some(EncodedOutputEvent::Data(chunk)) => Some(chunk),
            _ => None,
        };
        let audio_data = match &self.next_audio {
            Some(EncodedOutputEvent::Data(chunk)) => Some(chunk),
            _ => None,
        };

        match (&video_data, &audio_data) {
            // try to wait for both audio and video events to be ready
            (Some(video_chunk), Some(audio_chunk)) => {
                if audio_chunk.pts > video_chunk.pts {
                    self.next_video.take().ok_or(InterleaveResult::Continue)
                } else {
                    self.next_audio.take().ok_or(InterleaveResult::Continue)
                }
            }
            // read audio if there is no way to get video event
            (None, Some(_)) if self.video_stream.is_none() => {
                self.next_audio.take().ok_or(InterleaveResult::Continue)
            }
            // read video if there is no way to get audio event
            (Some(_), None) if self.audio_stream.is_none() => {
                self.next_video.take().ok_or(InterleaveResult::Continue)
            }
            (None, None) => Err(InterleaveResult::Abort),
            // we can't do anything here, but there are still receivers
            // that can return something in the next loop.
            //
            // I don't think this can ever happen
            (_, _) => Err(InterleaveResult::Continue),
        }
    }

    async fn send_chunk_to_peer(&mut self, chunk: EncodedOutputChunk) -> Result<(), WhepError> {
        let stream = match chunk.kind {
            MediaKind::Video(_) => self.video_stream.as_mut(),
            MediaKind::Audio(_) => self.audio_stream.as_mut(),
        };

        let Some(stream) = stream else {
            error!(kind=?chunk.kind, "No stream of this kind");
            return Ok(());
        };

        match stream.payloader.payload(chunk) {
            Ok(rtp_packets) => {
                for rtp_packet in rtp_packets {
                    if let Err(err) = stream.track.write_rtp(&rtp_packet.packet).await {
                        return Err(WhepError::RtpWriteError(err));
                    }
                    trace!(?rtp_packet, "RTP packet written to track");
                }
            }
            Err(err) => {
                return Err(WhepError::PayloadingError(err));
            }
        }
        Ok(())
    }
}
