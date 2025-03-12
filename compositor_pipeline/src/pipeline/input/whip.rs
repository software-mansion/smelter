use rtp::codecs::{h264::H264Packet, vp8::Vp8Packet};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tokio::sync::mpsc;
use webrtc::{rtp_transceiver::rtp_codec::RTPCodecType, track::track_remote::TrackRemote};

use depayloader::{Depayloader, RolloverState, VideoDepayloader};
use tracing::{error, warn, Span};

use crate::{
    audio_mixer::InputSamples,
    pipeline::{
        decoder::{self, start_video_decoder_thread, VideoDecoderOptions},
        types::EncodedChunk,
        whip_whep::{bearer_token::generate_token, WhipInputConnectionOptions, WhipWhepState},
        PipelineCtx, VideoDecoder,
    },
    queue::PipelineEvent,
};
use compositor_render::{Frame, InputId};
use crossbeam_channel::Sender;
use tracing::{debug, span, Level};

use super::{Input, InputInitInfo};

pub mod depayloader;

#[derive(Debug, thiserror::Error)]
pub enum WhipReceiverError {
    #[error("WHIP WHEP server is not running, cannot start WHIP input")]
    WhipWhepServerNotRunning,
}

#[derive(Debug, Clone)]
pub struct WhipReceiverOptions {
    pub video: Option<InputVideoStream>,
    pub audio: Option<InputAudioStream>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputVideoStream {
    pub options: decoder::VideoDecoderOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputAudioStream {
    pub options: decoder::OpusDecoderOptions,
}

pub struct WhipReceiver {
    whip_whep_state: Arc<WhipWhepState>,
    input_id: InputId,
}

impl WhipReceiver {
    pub(super) fn start_new_input(
        input_id: &InputId,
        pipeline_ctx: &PipelineCtx,
        frame_sender: Sender<PipelineEvent<Frame>>,
        input_samples_sender: Sender<PipelineEvent<InputSamples>>,
    ) -> Result<(Input, InputInitInfo), WhipReceiverError> {
        if !pipeline_ctx.start_whip_whep {
            return Err(WhipReceiverError::WhipWhepServerNotRunning);
        }
        let bearer_token = generate_token();
        let whip_whep_state = pipeline_ctx.whip_whep_state.clone();

        let (video_sender_async, video_chunk_receiver) = {
            let (async_sender, async_receiver) = mpsc::channel(100);
            let (sync_sender, sync_receiver) = crossbeam_channel::bounded(100);
            let span = span!(
                Level::INFO,
                "WHIP server video async-to-sync bridge",
                input_id = input_id.to_string()
            );
            Self::start_forwarding_thread(async_receiver, sync_sender, span);
            (async_sender, sync_receiver)
        };

        let (audio_sender_async, audio_chunk_receiver) = {
            let (async_sender, async_receiver) = mpsc::channel(100);
            let (sync_sender, sync_receiver) = crossbeam_channel::bounded(100);
            let span = span!(
                Level::INFO,
                "WHIP server audio async-to-sync bridge",
                input_id = input_id.to_string(),
            );
            Self::start_forwarding_thread(async_receiver, sync_sender, span);
            (async_sender, sync_receiver)
        };

        let mut input_connections = whip_whep_state.input_connections.lock().unwrap();
        input_connections.insert(
            input_id.clone(),
            WhipInputConnectionOptions {
                audio_sender: audio_sender_async.clone(),
                video_sender: video_sender_async.clone(),
                bearer_token: Some(bearer_token.clone()),
                peer_connection: None,
                start_time_vid: None,
                start_time_aud: None,
                frame_sender,
                input_samples_sender,
                video_chunk_receiver,
                audio_chunk_receiver,
            },
        );

        Ok((
            Input::Whip(Self {
                whip_whep_state: whip_whep_state.clone(),
                input_id: input_id.clone(),
            }),
            InputInitInfo::Whip { bearer_token },
        ))
    }

    fn start_forwarding_thread(
        mut async_receiver: mpsc::Receiver<PipelineEvent<EncodedChunk>>,
        sync_sender: Sender<PipelineEvent<EncodedChunk>>,
        span: Span,
    ) {
        thread::spawn(move || {
            let _span = span.entered();
            loop {
                let Some(chunk) = async_receiver.blocking_recv() else {
                    debug!("Closing WHIP async-to-sync bridge.");
                    break;
                };

                if let Err(err) = sync_sender.send(chunk) {
                    debug!("Failed to send Encoded Chunk. Channel closed: {:?}", err);
                    break;
                }
            }
        });
    }
}

impl Drop for WhipReceiver {
    fn drop(&mut self) {
        let mut connections = self.whip_whep_state.input_connections.lock().unwrap();
        if let Some(connection) = connections.get_mut(&self.input_id) {
            if let Some(peer_connection) = connection.peer_connection.clone() {
                let input_id = self.input_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        error!("Cannot close peer_connection for {:?}: {:?}", input_id, err);
                    };
                });
            }
        }
        connections.remove(&self.input_id);
    }
}

pub async fn process_track_stream(
    track: Arc<TrackRemote>,
    state: Arc<PipelineCtx>,
    input_id: InputId,
    sender: mpsc::Sender<PipelineEvent<EncodedChunk>>,
    codecs: Arc<HashMap<u8, String>>,
) {
    let input_id_clone = input_id.clone();
    let track_kind = track.kind();
    let time_elapsed_from_input_start = state
        .whip_whep_state
        .get_time_elapsed_from_input_start(input_id.clone(), track_kind);

    //TODO send PipelineEvent::NewPeerConnection to reset queue and decoder(drop remaining frames from previous stream)

    let mut first_pts_current_stream = None;
    let mut depayloader: Option<Arc<Mutex<Depayloader>>> = None;
    let mut flag = true;
    let frame_sender = state
        .whip_whep_state
        .input_connections
        .lock()
        .unwrap()
        .get(&input_id_clone)
        .unwrap()
        .frame_sender
        .clone();

    let chunks_receiver = state
        .whip_whep_state
        .input_connections
        .lock()
        .unwrap()
        .get(&input_id_clone)
        .unwrap()
        .video_chunk_receiver
        .clone();

    while let Ok((rtp_packet, _)) = track.read_rtp().await {
        if flag && track_kind == RTPCodecType::Video {
            flag = false;

            //dynamically coose codec
            let (video_decoder, video_depayloader) =
                parse_negotiated_video_codec(codecs.clone(), rtp_packet.header.payload_type);
            depayloader = Some(Arc::new(Mutex::new(Depayloader {
                video: Some(video_depayloader),
                audio: None,
            })));

            let _ = start_video_decoder_thread(
                video_decoder,
                &state,
                chunks_receiver.clone(),
                frame_sender.clone(),
                input_id_clone.clone(),
            );

            // depayloader = Some(Arc::new(Mutex::new(Depayloader {video: Some(VideoDepayloader::H264 { depayloader: H264Packet::default(), buffer: vec![], rollover_state:RolloverState::default() }), audio: None })));
        } else if track_kind == RTPCodecType::Audio {
            continue;
        }

        let chunks = match depayloader
            .clone()
            .unwrap()
            .lock()
            .unwrap()
            .depayload(rtp_packet, track_kind)
        {
            Ok(chunks) => chunks,
            Err(err) => {
                warn!("RTP depayloading error: {err:?}");
                continue;
            }
        };

        if let Some(first_chunk) = chunks.first() {
            first_pts_current_stream.get_or_insert(first_chunk.pts);
        }

        for mut chunk in chunks {
            chunk.pts = chunk.pts + time_elapsed_from_input_start.unwrap_or(Duration::ZERO)
                - first_pts_current_stream.unwrap_or(Duration::ZERO);
            if let Err(e) = sender.send(PipelineEvent::Data(chunk)).await {
                debug!("Failed to send audio RTP packet: {e}");
            }
        }
    }
}

fn parse_negotiated_video_codec(
    codecs: Arc<HashMap<u8, String>>,
    payload_type: u8,
) -> (VideoDecoderOptions, VideoDepayloader) {
    match codecs.get(&payload_type) {
        Some(val) if val == &"video/H264".to_string() => (
            VideoDecoderOptions {
                decoder: VideoDecoder::FFmpegH264,
            },
            VideoDepayloader::H264 {
                depayloader: H264Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            },
        ),
        Some(val) if val == &"video/VP8".to_string() => (
            VideoDecoderOptions {
                decoder: VideoDecoder::FFmpegVp8,
            },
            VideoDepayloader::VP8 {
                depayloader: Vp8Packet::default(),
                buffer: vec![],
                rollover_state: RolloverState::default(),
            },
        ),
        _ => unreachable!(),
    }
}
