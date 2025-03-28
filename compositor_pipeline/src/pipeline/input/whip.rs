use start_decoders::WhipInputDecoders;
use std::{sync::Arc, thread, time::Duration};
use tokio::sync::mpsc;
use tracing::{error, span, warn, Level};
use webrtc::track::track_remote::TrackRemote;

use crate::{
    audio_mixer::InputSamples,
    pipeline::{
        decoder,
        types::EncodedChunk,
        whip_whep::{bearer_token::generate_token, WhipInputConnectionState, WhipInputState},
        PipelineCtx,
    },
    queue::PipelineEvent,
};
use compositor_render::{Frame, InputId};
use crossbeam_channel::{Receiver, Sender};
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

pub struct WhipInput {
    whip_inputs_state: WhipInputState,
    input_id: InputId,
}

#[derive(Debug, Clone)]
pub struct DecodedDataSender {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputSamples>>,
}

impl WhipInput {
    pub(super) fn start_new_input(
        input_id: &InputId,
        pipeline_ctx: &PipelineCtx,
        frame_sender: Sender<PipelineEvent<Frame>>,
        input_samples_sender: Sender<PipelineEvent<InputSamples>>,
    ) -> Result<(Input, InputInitInfo), WhipReceiverError> {
        let Some(state) = &pipeline_ctx.whip_inputs_state else {
            return Err(WhipReceiverError::WhipWhepServerNotRunning);
        };
        let bearer_token = generate_token();

        let decoded_data_sender = DecodedDataSender {
            frame_sender,
            input_samples_sender,
        };

        state.add_input(
            input_id,
            WhipInputConnectionState {
                bearer_token: Some(bearer_token.clone()),
                peer_connection: None,
                start_time_video: None,
                start_time_audio: None,
                decoded_data_sender,
            },
        );

        Ok((
            Input::Whip(Self {
                whip_inputs_state: state.clone(),
                input_id: input_id.clone(),
            }),
            InputInitInfo::Whip { bearer_token },
        ))
    }
}

impl Drop for WhipInput {
    fn drop(&mut self) {
        self.whip_inputs_state.close_input(&self.input_id);
    }
}

pub fn start_forwarding_thread(
    input_id: InputId,
) -> (
    mpsc::Sender<PipelineEvent<EncodedChunk>>,
    Receiver<PipelineEvent<EncodedChunk>>,
) {
    let (whip_client_to_bridge_sender, mut whip_client_to_bridge_receiver) = mpsc::channel(50);
    let (bridge_to_decoder_sender, bridge_to_decoder_receiver) = crossbeam_channel::bounded(50);
    thread::spawn(move || {
        let _span: span::EnteredSpan = span!(
            Level::INFO,
            "WHIP server async-to-sync bridge",
            input_id = input_id.to_string(),
        )
        .entered();
        loop {
            let Some(chunk) = whip_client_to_bridge_receiver.blocking_recv() else {
                debug!("Closing WHIP async-to-sync bridge.");
                break;
            };

            if let Err(err) = bridge_to_decoder_sender.send(chunk) {
                debug!("Failed to send Encoded Chunk. Channel closed: {:?}", err);
                break;
            }
        }
    });
    (whip_client_to_bridge_sender, bridge_to_decoder_receiver)
}

pub async fn process_track_stream(
    track: Arc<TrackRemote>,
    inputs: WhipInputState,
    input_id: InputId,
    payload_type_map: WhipInputDecoders,
) {
    let track_kind = track.kind();
    let time_elapsed_from_input_start =
        inputs.get_time_elapsed_from_input_start(input_id.clone(), track_kind);

    let mut first_pts_current_stream = None;

    while let Ok((rtp_packet, _)) = track.read_rtp().await {
        if let Some((sender, depayloader)) = payload_type_map.get(&rtp_packet.header.payload_type) {
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
                if let Err(e) = sender.send(PipelineEvent::Data(chunk)).await {
                    debug!("Failed to send audio RTP packet: {e}");
                }
            }
        };
    }
}
