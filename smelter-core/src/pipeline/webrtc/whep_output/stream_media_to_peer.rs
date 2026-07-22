use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use smelter_render::error::ErrorStack;
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{error, info, trace, warn};
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
            let Some(chunk) = self.sender.resolve_next_chunk().await else {
                return;
            };

            if self.should_close.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            if let Err(err) = self.sender.send_chunk_to_peer(chunk).await {
                error!("{}", ErrorStack::new(&err).into_string());
                break;
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
    next_video: Option<EncodedOutputChunk>,
    next_audio: Option<EncodedOutputChunk>,
}

impl InterleavedPacketSender {
    async fn resolve_next_chunk(&mut self) -> Option<EncodedOutputChunk> {
        loop {
            let needs_video = self.video_stream.is_some() && self.next_video.is_none();
            let needs_audio = self.audio_stream.is_some() && self.next_audio.is_none();

            match (needs_video, needs_audio) {
                (true, true) => {
                    tokio::select! {
                        result = self.video_stream.as_mut().unwrap().receiver.recv() => {
                            self.handle_video_read_result(result)
                        },
                        result = self.audio_stream.as_mut().unwrap().receiver.recv() => {
                            self.handle_audio_read_result(result)
                        },
                    };
                }
                (true, false) => {
                    let result = self.video_stream.as_mut().unwrap().receiver.recv().await;
                    self.handle_video_read_result(result);
                }
                (false, true) => {
                    let result = self.audio_stream.as_mut().unwrap().receiver.recv().await;
                    self.handle_audio_read_result(result);
                }
                (false, false) => return self.resolve_from_state(),
            }
        }
    }

    fn handle_video_read_result(&mut self, result: Result<EncodedOutputEvent, RecvError>) {
        match result {
            Ok(EncodedOutputEvent::Data(chunk)) => self.next_video = Some(chunk),
            Ok(EncodedOutputEvent::AudioEOS) => error!("Unexpected AudioEOS on video track"),
            Err(RecvError::Lagged(count)) => warn!(count, "Video stream on WHEP output lagged"),
            Err(RecvError::Closed) | Ok(EncodedOutputEvent::VideoEOS) => {
                info!("Received video EOS event on WHEP output");
                self.video_stream = None;
            }
        }
    }

    fn handle_audio_read_result(&mut self, result: Result<EncodedOutputEvent, RecvError>) {
        match result {
            Ok(EncodedOutputEvent::Data(chunk)) => self.next_audio = Some(chunk),
            Ok(EncodedOutputEvent::VideoEOS) => error!("Unexpected VideoEOS on audio track"),
            Err(RecvError::Lagged(count)) => warn!(count, "Audio stream on WHEP output lagged"),
            Err(RecvError::Closed) | Ok(EncodedOutputEvent::AudioEOS) => {
                info!("Received audio EOS event on WHEP output");
                self.audio_stream = None;
            }
        }
    }

    fn resolve_from_state(&mut self) -> Option<EncodedOutputChunk> {
        match (&self.next_video, &self.next_audio) {
            (Some(video_chunk), Some(audio_chunk)) => {
                if audio_chunk.pts > video_chunk.pts {
                    self.next_video.take()
                } else {
                    self.next_audio.take()
                }
            }
            (None, Some(_)) => self.next_audio.take(),
            (Some(_), None) => self.next_video.take(),
            (None, None) => None,
        }
    }

    async fn send_chunk_to_peer(&mut self, chunk: EncodedOutputChunk) -> Result<(), WhepError> {
        let kind = chunk.kind;
        let stream = match kind {
            MediaKind::Video(_) => self.video_stream.as_mut(),
            MediaKind::Audio(_) => self.audio_stream.as_mut(),
        };

        let Some(stream) = stream else {
            error!(?kind, "No stream of this kind");
            return Ok(());
        };

        match stream.payloader.payload(chunk) {
            Ok(rtp_packets) => {
                for rtp_packet in rtp_packets {
                    if let Err(err) = stream.track.write_rtp(&rtp_packet.packet).await {
                        return Err(WhepError::RtpWriteError(err));
                    }
                    trace!(?rtp_packet, ?kind, "RTP packet written to track");
                }
            }
            Err(err) => {
                return Err(WhepError::PayloadingError(err));
            }
        }
        Ok(())
    }
}
