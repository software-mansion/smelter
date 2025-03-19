use crate::{
    pipeline::{
        self,
        decoder::{
            start_audio_decoder_thread, start_video_decoder_thread, AudioDecoderOptions,
            OpusDecoderOptions, VideoDecoderOptions,
        },
        input::whip::{
            depayloader::{AudioDepayloader, RolloverState, VideoDepayloader},
            start_forwarding_thread,
        },
        whip_whep::error::WhipServerError,
        EncodedChunk, PipelineCtx,
    },
    queue::PipelineEvent,
};

use compositor_render::InputId;
use rtp::codecs::{h264::H264Packet, opus::OpusPacket, vp8::Vp8Packet};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{self, Sender};
use tracing::{span, Level};
use webrtc::peer_connection::RTCPeerConnection;

#[derive(Clone, Debug)]
struct CodecsPayloadTypes {
    h264: Vec<u8>,
    vp8: Vec<u8>,
    opus: Vec<u8>,
}

pub async fn start_decoders_threads(
    pipeline_ctx: Arc<PipelineCtx>,
    input_id: InputId,
) -> Result<
    (
        HashMap<u8, (Sender<PipelineEvent<EncodedChunk>>, VideoDepayloader)>,
        HashMap<u8, (Sender<PipelineEvent<EncodedChunk>>, AudioDepayloader)>,
    ),
    WhipServerError,
> {
    let mut video_decoder_map = HashMap::new();
    let mut audio_decoder_map = HashMap::new();

    let input_components = pipeline_ctx
        .whip_whep_state
        .get_input_connection_options(input_id.clone())?;

    let Some(peer_connection) = input_components.peer_connection else {
        return Err(WhipServerError::InternalError(
            "Peer connection has not been initialized!".to_string(),
        ));
    };

    let negotiated_codecs = get_codec_map(peer_connection).await;
    let decoded_data_sender = input_components.decoded_data_sender.clone();

    let input_id_clone = input_id.clone();

    if !negotiated_codecs.h264.is_empty() {
        let (async_sender, async_receiver) = mpsc::channel(100);
        let (sync_sender, sync_receiver) = crossbeam_channel::bounded(100);

        let span = span!(
            Level::INFO,
            "WHIP server video async-to-sync bridge",
            input_id = input_id.to_string()
        );

        start_forwarding_thread(async_receiver, sync_sender, span);
        start_video_decoder_thread(
            VideoDecoderOptions {
                decoder: pipeline::VideoDecoder::FFmpegH264,
            },
            &pipeline_ctx,
            sync_receiver,
            decoded_data_sender.frame_sender.clone(),
            input_id_clone.clone(),
            false,
        )?;
        let depayloader = VideoDepayloader::H264 {
            depayloader: H264Packet::default(),
            buffer: vec![],
            rollover_state: RolloverState::default(),
        };
        for payload_type in negotiated_codecs.h264 {
            video_decoder_map.insert(payload_type, (async_sender.clone(), depayloader.clone()));
        }
    }

    if !negotiated_codecs.vp8.is_empty() {
        let (async_sender, async_receiver) = mpsc::channel(100);
        let (sync_sender, sync_receiver) = crossbeam_channel::bounded(100);

        let span = span!(
            Level::INFO,
            "WHIP server video async-to-sync bridge",
            input_id = input_id.to_string()
        );

        start_forwarding_thread(async_receiver, sync_sender, span);
        start_video_decoder_thread(
            VideoDecoderOptions {
                decoder: pipeline::VideoDecoder::FFmpegVp8,
            },
            &pipeline_ctx,
            sync_receiver,
            decoded_data_sender.frame_sender.clone(),
            input_id_clone.clone(),
            false,
        )?;
        let depayloader = VideoDepayloader::VP8 {
            depayloader: Vp8Packet::default(),
            buffer: vec![],
            rollover_state: RolloverState::default(),
        };
        for payload_type in negotiated_codecs.vp8 {
            video_decoder_map.insert(payload_type, (async_sender.clone(), depayloader.clone()));
        }
    }

    if !negotiated_codecs.opus.is_empty() {
        let (async_sender, async_receiver) = mpsc::channel(100);
        let (sync_sender, sync_receiver) = crossbeam_channel::bounded(100);

        let span = span!(
            Level::INFO,
            "WHIP server video async-to-sync bridge",
            input_id = input_id.to_string()
        );

        start_forwarding_thread(async_receiver, sync_sender, span);
        start_audio_decoder_thread(
            AudioDecoderOptions::Opus(OpusDecoderOptions {
                forward_error_correction: false,
            }),
            pipeline_ctx.mixing_sample_rate,
            sync_receiver,
            decoded_data_sender.input_samples_sender.clone(),
            input_id_clone.clone(),
            false,
        )?;

        let depayloader = AudioDepayloader::Opus {
            depayloader: OpusPacket,
            rollover_state: RolloverState::default(),
        };
        for payload_type in negotiated_codecs.opus {
            audio_decoder_map.insert(payload_type, (async_sender.clone(), depayloader.clone()));
        }
    }

    if video_decoder_map.is_empty() && audio_decoder_map.is_empty() {
        return Err(WhipServerError::CodecNegotiationError(
            "None of negotiated codecs are supported in the Smelter!".to_string(),
        ));
    }
    Ok((video_decoder_map, audio_decoder_map))
}

async fn get_codec_map(peer_connection: Arc<RTCPeerConnection>) -> CodecsPayloadTypes {
    let mut codec_payload_types = CodecsPayloadTypes {
        h264: vec![],
        vp8: vec![],
        opus: vec![],
    };

    let transceivers = peer_connection.get_transceivers().await;

    for transceiver in transceivers {
        let receiver = transceiver.receiver().await;
        let codecs = receiver.get_parameters().await.codecs;

        for codec in codecs {
            // codec_map.insert(codec.payload_type, codec.capability.mime_type);
            match codec.capability.mime_type.as_str() {
                "video/H264" => codec_payload_types.h264.push(codec.payload_type),
                "video/VP8" => codec_payload_types.vp8.push(codec.payload_type),
                "audio/opus" => codec_payload_types.opus.push(codec.payload_type),
                _ => {}
            }
        }
    }
    codec_payload_types
}
