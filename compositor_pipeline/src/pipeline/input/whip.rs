use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tokio::sync::mpsc;
use webrtc::{rtp_transceiver::rtp_codec::RTPCodecType, track::track_remote::TrackRemote};

use depayloader::{AudioDepayloader, Depayloader, VideoDepayloader};
use tracing::{error, warn, Span};

use crate::{
    audio_mixer::InputSamples,
    pipeline::{
        decoder,
        types::EncodedChunk,
        whip_whep::{bearer_token::generate_token, WhipInputConnectionOptions, WhipWhepState},
        PipelineCtx,
    },
    queue::PipelineEvent,
};
use compositor_render::{Frame, InputId};
use crossbeam_channel::Sender;
use tracing::debug;

use super::{Input, InputInitInfo};

pub mod depayloader;
pub mod start_decoders;

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

#[derive(Debug, Clone)]
pub struct DecodedDataSender {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputSamples>>,
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

        let decoded_data_sender = DecodedDataSender {
            frame_sender,
            input_samples_sender,
        };

        let mut input_connections = whip_whep_state.input_connections.lock().unwrap();
        input_connections.insert(
            input_id.clone(),
            WhipInputConnectionOptions {
                bearer_token: Some(bearer_token.clone()),
                peer_connection: None,
                start_time_vid: None,
                start_time_aud: None,
                decoded_data_sender,
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

pub fn start_forwarding_thread(
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

pub async fn process_track_stream(
    track: Arc<TrackRemote>,
    state: Arc<PipelineCtx>,
    input_id: InputId,
    video_decoder_map: HashMap<u8, (mpsc::Sender<PipelineEvent<EncodedChunk>>, VideoDepayloader)>,
    audio_decoder_map: HashMap<u8, (mpsc::Sender<PipelineEvent<EncodedChunk>>, AudioDepayloader)>,
) {
    let track_kind = track.kind();
    let time_elapsed_from_input_start = state
        .whip_whep_state
        .get_time_elapsed_from_input_start(input_id.clone(), track_kind);

    //TODO send PipelineEvent::NewPeerConnection to reset queue and decoder(drop remaining frames from previous stream)
    let mut first_pts_current_stream = None;
    let mut flag = true;

    let depayloader = Arc::new(Mutex::new(Depayloader {
        video: None,
        audio: None,
    }));

    let mut sender_global = None;

    while let Ok((rtp_packet, _)) = track.read_rtp().await {
        if flag && track_kind == RTPCodecType::Video {
            flag = false;

            if let Some((sender, video_depayloader)) =
                video_decoder_map.get(&rtp_packet.header.payload_type)
            {
                sender_global = Some(sender);
                depayloader.lock().unwrap().video = Some(video_depayloader.clone());
            }
        } else if flag && track_kind == RTPCodecType::Audio {
            flag = false;

            if let Some((sender, audio_depayloader)) =
                audio_decoder_map.get(&rtp_packet.header.payload_type)
            {
                sender_global = Some(sender);
                depayloader.lock().unwrap().audio = Some(audio_depayloader.clone());
            }
        }

        let chunks = match depayloader
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
            if let Err(e) = sender_global
                .unwrap()
                .send(PipelineEvent::Data(chunk))
                .await
            {
                debug!("Failed to send audio RTP packet: {e}");
            }
        }
    }
}
